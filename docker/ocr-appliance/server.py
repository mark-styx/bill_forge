"""BillForge tenant-side OCR appliance (refs #421).

Runs INSIDE the tenant boundary. The BillForge SaaS plane POSTs invoice
bytes here, the appliance runs Tesseract locally, and only the
extracted structured JSON crosses back to the SaaS plane. Image bytes
never leave the tenant network.

Wire protocol matches
backend/crates/invoice-capture/src/ocr/private_inference.rs:
  POST <configured ocr_endpoint_url>
      Content-Type: application/octet-stream
      body = raw image bytes (PNG/JPEG/TIFF) or PDF bytes
      -> 200 application/json shaped as OcrExtractionResult
  GET  <ocr_endpoint_url>/health -> 200 {"status":"ok"}

The Rust health probe builds the URL by appending ``/health`` to the
configured endpoint, so we expose ``/health`` AND ``/ocr/health`` to
cover both common tenant deployment shapes (``http://host/`` and
``http://host/ocr``). ``/healthz`` is for the Docker HEALTHCHECK.
"""

from __future__ import annotations

import io
import logging
import os
import re
import secrets
import time
import uuid
from datetime import date
from typing import Any, Iterable, Optional

from fastapi import FastAPI, Header, HTTPException, Request
from fastapi.responses import JSONResponse

logger = logging.getLogger("ocr_appliance")
logging.basicConfig(
    level=os.environ.get("OCR_APPLIANCE_LOG_LEVEL", "INFO"),
    format="%(asctime)s %(levelname)s %(name)s %(message)s",
)


def get_max_bytes() -> int:
    return int(os.environ.get("OCR_APPLIANCE_MAX_BYTES", str(25 * 1024 * 1024)))


def get_shared_secret() -> Optional[str]:
    secret = os.environ.get("OCR_APPLIANCE_SHARED_SECRET")
    return secret if secret else None


def get_max_pdf_pages() -> int:
    return int(os.environ.get("OCR_APPLIANCE_MAX_PDF_PAGES", "5"))


app = FastAPI(title="BillForge OCR Appliance", version="1.0.0")


# ---------------------------------------------------------------------------
# Response shape (mirrors billforge_core::domain::OcrExtractionResult)
# ---------------------------------------------------------------------------


def empty_field() -> dict[str, Any]:
    return {"value": None, "confidence": 0.0, "bounding_box": None, "source_text": None}


def field(value: Any, confidence: float, source_text: Optional[str] = None) -> dict[str, Any]:
    return {
        "value": value,
        "confidence": confidence,
        "bounding_box": None,
        "source_text": source_text,
    }


def empty_result(raw_text: str, processing_time_ms: int) -> dict[str, Any]:
    return {
        "invoice_number": empty_field(),
        "invoice_date": empty_field(),
        "due_date": empty_field(),
        "vendor_name": empty_field(),
        "vendor_address": empty_field(),
        "subtotal": empty_field(),
        "tax_amount": empty_field(),
        "total_amount": empty_field(),
        "currency": empty_field(),
        "po_number": empty_field(),
        "line_items": [],
        "raw_text": raw_text,
        "processing_time_ms": processing_time_ms,
    }


# ---------------------------------------------------------------------------
# Payload decoding
# ---------------------------------------------------------------------------


def is_pdf(data: bytes) -> bool:
    return data[:4] == b"%PDF"


def open_images(data: bytes) -> list[Any]:
    from PIL import Image

    if is_pdf(data):
        try:
            from pdf2image import convert_from_bytes
        except Exception as e:
            raise HTTPException(status_code=500, detail=f"pdf2image unavailable: {e}")
        try:
            return convert_from_bytes(data, first_page=1, last_page=get_max_pdf_pages())
        except Exception as e:
            raise HTTPException(status_code=400, detail=f"failed to render PDF: {e}")

    try:
        img = Image.open(io.BytesIO(data))
        img.load()
        return [img]
    except Exception as e:
        raise HTTPException(status_code=400, detail=f"unrecognized image payload: {e}")


# ---------------------------------------------------------------------------
# Tesseract (layout-aware via image_to_data)
# ---------------------------------------------------------------------------


def run_tesseract(images: Iterable[Any]) -> tuple[str, float]:
    try:
        import pytesseract
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"pytesseract unavailable: {e}")

    page_texts: list[str] = []
    confidences: list[float] = []

    for img in images:
        try:
            data = pytesseract.image_to_data(img, output_type=pytesseract.Output.DICT)
        except pytesseract.TesseractNotFoundError as e:
            raise HTTPException(status_code=500, detail=f"tesseract binary missing: {e}")
        except Exception as e:
            raise HTTPException(status_code=500, detail=f"tesseract failed: {e}")

        n = len(data.get("text", []))
        current_line_id: Optional[tuple[int, int, int]] = None
        line_words: list[str] = []
        out_lines: list[str] = []

        for i in range(n):
            word = (data["text"][i] or "").strip()
            if not word:
                continue
            line_id = (
                int(data["page_num"][i]),
                int(data["block_num"][i]),
                int(data["line_num"][i]),
            )
            if current_line_id is None:
                current_line_id = line_id
            if line_id != current_line_id:
                if line_words:
                    out_lines.append(" ".join(line_words))
                line_words = []
                current_line_id = line_id
            line_words.append(word)
            try:
                c = float(data["conf"][i])
                if c >= 0:
                    confidences.append(c)
            except (TypeError, ValueError):
                pass

        if line_words:
            out_lines.append(" ".join(line_words))
        page_texts.append("\n".join(out_lines))

    full_text = "\n\n".join(page_texts)
    avg_conf = (sum(confidences) / len(confidences) / 100.0) if confidences else 0.0
    return full_text, avg_conf


# ---------------------------------------------------------------------------
# Field extraction (lightweight regex, matches the in-tree tesseract module
# closely enough for downstream confidence/post-processing to be happy)
# ---------------------------------------------------------------------------


INVOICE_NUMBER_PATTERNS = [
    re.compile(r"(?i)invoice\s*number\s*:?\s*([A-Z0-9\-]{3,})"),
    re.compile(r"(?i)invoice\s*#?\s*:?\s*([A-Z0-9\-]{3,})"),
    re.compile(r"(?i)inv\s*#?\s*:?\s*([A-Z0-9\-]{3,})"),
]

PO_NUMBER_PATTERNS = [
    re.compile(r"(?i)\bpo\s*#?\s*:?\s*([A-Z0-9\-]{3,})"),
    re.compile(r"(?i)purchase\s*order\s*:?\s*([A-Z0-9\-]{3,})"),
]

AMOUNT_RE = re.compile(r"([\d,]+\.\d{2})")

DATE_RES = [
    (re.compile(r"(\d{4})[-/](\d{1,2})[-/](\d{1,2})"), "ymd"),
    (re.compile(r"(\d{1,2})[-/](\d{1,2})[-/](\d{4})"), "mdy"),
]


def _first_match(patterns: list[re.Pattern[str]], text: str) -> Optional[str]:
    for pat in patterns:
        m = pat.search(text)
        if m:
            return m.group(1).strip()
    return None


def extract_invoice_number(text: str) -> dict[str, Any]:
    val = _first_match(INVOICE_NUMBER_PATTERNS, text)
    return field(val, 0.85) if val else empty_field()


def extract_po_number(text: str) -> dict[str, Any]:
    val = _first_match(PO_NUMBER_PATTERNS, text)
    return field(val, 0.8) if val else empty_field()


def extract_currency(text: str) -> dict[str, Any]:
    upper = text.upper()
    if "$" in text or "USD" in upper:
        return field("USD", 0.9)
    if "€" in text or "EUR" in upper:
        return field("EUR", 0.9)
    if "£" in text or "GBP" in upper:
        return field("GBP", 0.9)
    return empty_field()


def extract_amount(text: str, keywords: list[str]) -> dict[str, Any]:
    lower = text.lower()
    for kw in keywords:
        i = lower.find(kw)
        if i < 0:
            continue
        m = AMOUNT_RE.search(text, i)
        if m:
            try:
                return field(float(m.group(1).replace(",", "")), 0.75)
            except ValueError:
                continue
    return empty_field()


def parse_date(s: str) -> Optional[date]:
    for pat, order in DATE_RES:
        m = pat.search(s)
        if not m:
            continue
        try:
            if order == "ymd":
                return date(int(m.group(1)), int(m.group(2)), int(m.group(3)))
            return date(int(m.group(3)), int(m.group(1)), int(m.group(2)))
        except ValueError:
            continue
    return None


def extract_date(text: str, keywords: list[str]) -> dict[str, Any]:
    lower = text.lower()
    for kw in keywords:
        i = lower.find(kw)
        if i < 0:
            continue
        d = parse_date(text[i:])
        if d:
            return field(d.isoformat(), 0.8)
    d = parse_date(text)
    if d:
        return field(d.isoformat(), 0.5)
    return empty_field()


def extract_vendor_name(lines: list[str]) -> dict[str, Any]:
    for line in lines[:10]:
        s = line.strip()
        if not s or not any(c.isalpha() for c in s):
            continue
        lower = s.lower()
        if "invoice" in lower or "bill to" in lower or s.startswith("$"):
            continue
        if 3 < len(s) < 60:
            return field(s, 0.7)
    return empty_field()


def extract_fields(text: str) -> dict[str, Any]:
    lines = text.splitlines()
    return {
        "invoice_number": extract_invoice_number(text),
        "invoice_date": extract_date(text, ["invoice date", "date:", "inv date", "bill date"]),
        "due_date": extract_date(text, ["due date", "payment due", "due:", "pay by"]),
        "vendor_name": extract_vendor_name(lines),
        "vendor_address": empty_field(),
        "subtotal": extract_amount(text, ["subtotal", "sub total", "sub-total"]),
        "tax_amount": extract_amount(text, ["tax", "vat", "gst"]),
        "total_amount": extract_amount(
            text, ["total", "amount due", "grand total", "balance due"]
        ),
        "currency": extract_currency(text),
        "po_number": extract_po_number(text),
        "line_items": [],
    }


# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------


def check_auth(token: Optional[str]) -> None:
    secret = get_shared_secret()
    if secret is None:
        return
    if not token or not secrets.compare_digest(token, secret):
        raise HTTPException(status_code=401, detail="invalid appliance token")


@app.get("/healthz")
@app.get("/health")
@app.get("/ocr/health")
async def health() -> dict[str, str]:
    return {"status": "ok"}


@app.post("/ocr")
async def ocr(
    request: Request,
    x_appliance_token: Optional[str] = Header(default=None, alias="X-Appliance-Token"),
) -> JSONResponse:
    check_auth(x_appliance_token)

    max_bytes = get_max_bytes()
    cl = request.headers.get("content-length")
    if cl is not None:
        try:
            if int(cl) > max_bytes:
                raise HTTPException(status_code=413, detail="payload too large")
        except ValueError:
            pass

    body = await request.body()
    if len(body) > max_bytes:
        raise HTTPException(status_code=413, detail="payload too large")
    if not body:
        raise HTTPException(status_code=400, detail="empty request body")

    req_id = uuid.uuid4().hex[:12]
    start = time.perf_counter()

    images = open_images(body)
    text, _avg_conf = run_tesseract(images)

    duration_ms = int((time.perf_counter() - start) * 1000)

    result = empty_result(text, duration_ms)
    result.update(extract_fields(text))
    result["raw_text"] = text
    result["processing_time_ms"] = duration_ms

    # Privacy invariant: never log image bytes or extracted text.
    logger.info(
        "ocr.processed req_id=%s size=%d duration_ms=%d pages=%d",
        req_id,
        len(body),
        duration_ms,
        len(images),
    )
    return JSONResponse(content=result)
