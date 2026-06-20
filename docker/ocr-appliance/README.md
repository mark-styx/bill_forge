# BillForge OCR Appliance

Tenant-deployable OCR component for privacy-sensitive tenants (refs #421).

This container runs **inside the tenant boundary** (the tenant's VPC, on-prem
cluster, or single-tenant cloud instance). The BillForge SaaS plane POSTs
invoice bytes to it, Tesseract runs locally, and only the extracted
structured JSON crosses back to the SaaS plane. Image bytes never leave the
tenant network.

## What it is

- `python:3.11-slim` base with `tesseract-ocr`, `tesseract-ocr-eng`, and
  `poppler-utils` (for PDF rendering).
- FastAPI + uvicorn server (`server.py`).
- Wire protocol matches
  `backend/crates/invoice-capture/src/ocr/private_inference.rs` exactly —
  the BillForge backend is already wired to talk to this shape when a
  tenant's `tenant_private_inference.ocr_endpoint_url` points at the
  appliance.
- Layout-aware: uses `pytesseract.image_to_data` (TSV output) to
  reconstruct lines from word-level geometry. A higher-accuracy layout
  transformer (LayoutLM / Donut) is explicitly out of scope for this slice.

## Build & run

```
# from the BillForge repo root
docker build -f docker/Dockerfile.ocr-appliance -t billforge-ocr-appliance:dev .
docker run --rm -p 8088:8088 billforge-ocr-appliance:dev
curl -fsS http://127.0.0.1:8088/healthz
```

## API

| Method | Path           | Purpose                                                                 |
| ------ | -------------- | ----------------------------------------------------------------------- |
| POST   | `/ocr`         | Run OCR. `Content-Type: application/octet-stream`. Body = raw bytes.    |
| GET    | `/healthz`     | Docker `HEALTHCHECK` probe.                                              |
| GET    | `/health`      | Rust client probes `{ocr_endpoint_url}/health`; covers root deployment. |
| GET    | `/ocr/health`  | Same, for deployments where `ocr_endpoint_url` ends in `/ocr`.          |

`POST /ocr` accepts PNG / JPEG / TIFF / PDF (PDF first N pages, see
`OCR_APPLIANCE_MAX_PDF_PAGES`). The response JSON matches the Rust
`OcrExtractionResult` struct — top-level fields:

```
invoice_number, invoice_date, due_date, vendor_name, vendor_address,
subtotal, tax_amount, total_amount, currency, po_number,
line_items (array), raw_text (string), processing_time_ms (int)
```

Each scalar field is the `ExtractedField` shape
`{ value, confidence, bounding_box, source_text }`.

## Configuration

| Env var                          | Default        | Purpose                                                                                                       |
| -------------------------------- | -------------- | ------------------------------------------------------------------------------------------------------------- |
| `OCR_APPLIANCE_PORT`             | `8088`         | Port to bind.                                                                                                  |
| `OCR_APPLIANCE_MAX_BYTES`        | `26214400`     | Reject request bodies larger than this (returns 413). Default 25 MiB.                                          |
| `OCR_APPLIANCE_MAX_PDF_PAGES`    | `5`            | Maximum number of PDF pages rendered per request.                                                              |
| `OCR_APPLIANCE_SHARED_SECRET`    | (unset)        | If set, every request must carry `X-Appliance-Token: <secret>`. Checked with `secrets.compare_digest`.         |
| `OCR_APPLIANCE_LOG_LEVEL`        | `INFO`         | Python logging level.                                                                                          |

## Wiring it into BillForge

The backend side already exists. Once the appliance is reachable from the
BillForge SaaS plane, a tenant admin does two things:

1. Set the OCR endpoint URL on the tenant's private-inference row
   (migration `122_private_inference_config.sql` ships the column):

   ```sql
   INSERT INTO tenant_private_inference (tenant_id, enabled, ocr_endpoint_url, health_status)
   VALUES ($1, TRUE, 'https://ocr.tenant.example/ocr', 'unknown')
   ON CONFLICT (tenant_id) DO UPDATE
     SET enabled = TRUE,
         ocr_endpoint_url = EXCLUDED.ocr_endpoint_url;
   ```

2. Turn on the `local_ocr_required` policy via the admin settings API:

   ```
   PATCH /api/v1/settings/privacy
   Authorization: Bearer <admin token>
   { "local_ocr_required": true }
   ```

The Rust client (`backend/crates/invoice-capture/src/ocr/private_inference.rs`)
will probe `{ocr_endpoint_url}/health` and gate the policy via
`super::try_private_inference_ocr`.

## Data-flow diagram

```
                  TENANT BOUNDARY                                 SaaS PLANE
   +-----------------------------------------------+   ===   +------------------------+
   |                                               |   ===   |                        |
   |  +-----------------+      raw bytes (POST)    |   ===   |  BillForge backend     |
   |  | Invoice source  | -----------------------> |   ===   |  invoice-capture       |
   |  | (email/portal/  |                          |   ===   |   ocr::private_        |
   |  |  S3 ingest)     |                          |   ===   |   inference            |
   |  +-----------------+                          |   ===   |                        |
   |          |                                    |   ===   |   POSTs image bytes    |
   |          v                                    |   ===   |   to ocr_endpoint_url  |
   |  +-----------------+   structured JSON only   |   ===   |   <----------------+   |
   |  | OCR appliance   | <======================= |  XXXXX  |                    |   |
   |  | (this container)| =======================> |  XXXXX  | -------------------+   |
   |  | Tesseract +     |   OcrExtractionResult    |   ===   |                        |
   |  | layout (TSV)    |   (no image bytes)       |   ===   |   Persists structured  |
   |  +-----------------+                          |   ===   |   data only            |
   |                                               |   ===   |                        |
   +-----------------------------------------------+   ===   +------------------------+

   ===  arrows that cross the tenant<->SaaS boundary (structured data only)
   XXX  arrows that NEVER cross the boundary (raw bytes stay in-tenant)
```

The appliance is the only component that ever sees raw invoice bytes. The
BillForge SaaS plane sends bytes INTO the tenant (over the tenant's chosen
transport — VPN, mTLS, private link, etc.) and receives only the parsed JSON
back. The arrows marked `XXX` make explicit that the image bytes never
cross back to the SaaS plane.

## Deferred (intentionally out of scope for this slice)

- Per-module SaaS-vs-local routing for `categorization_endpoint_url` and
  `embeddings_endpoint_url`. The migration `122_private_inference_config.sql`
  reserves both columns but neither call site is wired yet. Adding the
  admin UI toggles is independent follow-up work.
- Replacing Tesseract's word/line/block geometry with a true layout
  transformer (LayoutLM / Donut). The current "layout model" deliverable
  is satisfied by `image_to_data` TSV output, which the response shape
  already carries via per-field `bounding_box` slots.
- Helm chart / k8s manifests, KMS-backed secret rotation, mTLS termination
  in the appliance itself (these are tenant-side concerns and outside the
  appliance's responsibility).

## Tests

```
cd docker/ocr-appliance
python -m pip install -r requirements.txt
pytest test_server.py -q
```

The happy-path test renders a PNG with PIL and expects the `tesseract`
binary on PATH. If it is not installed the test is skipped (not failed)
with a clear reason. Health, oversize-rejection, and shared-secret tests
run without `tesseract`.
