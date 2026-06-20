"""Tests for the BillForge OCR appliance.

Covers the contract called out in the plan for #421:
- response shape matches OcrExtractionResult (what private_inference.rs
  deserializes back on the SaaS side)
- happy path: rendered PNG recovers the embedded text
- /healthz returns 200
- payloads above OCR_APPLIANCE_MAX_BYTES return 413
- OCR_APPLIANCE_SHARED_SECRET, when set, gates requests on a constant-time
  header check
"""
from __future__ import annotations

import io
import shutil
from typing import Iterator

import pytest
from fastapi.testclient import TestClient

import server


def _png_with_text(text: str = "TEST INVOICE 1234") -> bytes:
    from PIL import Image, ImageDraw, ImageFont

    img = Image.new("RGB", (600, 100), color="white")
    draw = ImageDraw.Draw(img)
    font = None
    for candidate in ("DejaVuSans-Bold.ttf", "Arial.ttf", "Helvetica.ttf"):
        try:
            font = ImageFont.truetype(candidate, 36)
            break
        except OSError:
            continue
    if font is None:
        font = ImageFont.load_default()
    draw.text((10, 20), text, fill="black", font=font)
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    return buf.getvalue()


def _tesseract_available() -> bool:
    return shutil.which("tesseract") is not None


@pytest.fixture
def client(monkeypatch: pytest.MonkeyPatch) -> Iterator[TestClient]:
    monkeypatch.delenv("OCR_APPLIANCE_SHARED_SECRET", raising=False)
    monkeypatch.delenv("OCR_APPLIANCE_MAX_BYTES", raising=False)
    with TestClient(server.app) as c:
        yield c


EXPECTED_TOP_KEYS = {
    "invoice_number",
    "invoice_date",
    "due_date",
    "vendor_name",
    "vendor_address",
    "subtotal",
    "tax_amount",
    "total_amount",
    "currency",
    "po_number",
    "line_items",
    "raw_text",
    "processing_time_ms",
}

EXTRACTED_FIELD_KEYS = {"value", "confidence", "bounding_box", "source_text"}

EXTRACTED_FIELDS = [
    "invoice_number",
    "invoice_date",
    "due_date",
    "vendor_name",
    "vendor_address",
    "subtotal",
    "tax_amount",
    "total_amount",
    "currency",
    "po_number",
]


def test_health_endpoints(client: TestClient) -> None:
    for path in ("/healthz", "/health", "/ocr/health"):
        r = client.get(path)
        assert r.status_code == 200, path
        assert r.json() == {"status": "ok"}


@pytest.mark.skipif(
    not _tesseract_available(),
    reason="tesseract binary not on PATH (install tesseract-ocr to run this test)",
)
def test_ocr_happy_path_matches_extraction_result_shape(client: TestClient) -> None:
    payload = _png_with_text("TEST INVOICE 1234")
    r = client.post(
        "/ocr",
        content=payload,
        headers={"Content-Type": "application/octet-stream"},
    )
    assert r.status_code == 200, r.text
    body = r.json()

    assert set(body.keys()) == EXPECTED_TOP_KEYS
    for name in EXTRACTED_FIELDS:
        assert set(body[name].keys()) == EXTRACTED_FIELD_KEYS, name
    assert isinstance(body["line_items"], list)
    assert isinstance(body["raw_text"], str)
    assert isinstance(body["processing_time_ms"], int)

    normalised = body["raw_text"].upper().replace(" ", "").replace("\n", "")
    assert "1234" in normalised, body["raw_text"]


def test_oversize_payload_returns_413(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("OCR_APPLIANCE_MAX_BYTES", "1024")
    monkeypatch.delenv("OCR_APPLIANCE_SHARED_SECRET", raising=False)
    with TestClient(server.app) as c:
        oversize = b"x" * 4096
        r = c.post(
            "/ocr",
            content=oversize,
            headers={"Content-Type": "application/octet-stream"},
        )
        assert r.status_code == 413


def test_shared_secret_blocks_unauthenticated_requests(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("OCR_APPLIANCE_SHARED_SECRET", "swordfish")
    monkeypatch.delenv("OCR_APPLIANCE_MAX_BYTES", raising=False)
    with TestClient(server.app) as c:
        # No header -> 401.
        r = c.post(
            "/ocr",
            content=b"anything",
            headers={"Content-Type": "application/octet-stream"},
        )
        assert r.status_code == 401

        # Wrong header -> 401.
        r = c.post(
            "/ocr",
            content=b"anything",
            headers={
                "Content-Type": "application/octet-stream",
                "X-Appliance-Token": "wrong",
            },
        )
        assert r.status_code == 401

        # Correct header -> auth passes (we don't require tesseract here;
        # asserting "not 401" is what proves the constant-time check let
        # the request through).
        r = c.post(
            "/ocr",
            content=b"x",
            headers={
                "Content-Type": "application/octet-stream",
                "X-Appliance-Token": "swordfish",
            },
        )
        assert r.status_code != 401
