//! OCR pipeline data types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ──────────────────────────── Job Status ────────────────────────────

/// Status of an OCR processing job
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrJobStatus {
    /// Queued, waiting to be picked up
    Pending,
    /// Currently being processed
    Processing,
    /// OCR extraction completed successfully
    Completed,
    /// Processing failed (may be retried)
    Failed,
    /// Cancelled by user
    Cancelled,
}

impl OcrJobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

impl std::fmt::Display for OcrJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for OcrJobStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!("Unknown OcrJobStatus: {}", other)),
        }
    }
}

// ──────────────────────────── OCR Job ────────────────────────────

/// An OCR processing job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrJob {
    /// Unique job ID
    pub id: Uuid,
    /// Tenant this job belongs to
    pub tenant_id: Uuid,
    /// Document file ID in storage
    pub document_id: Uuid,
    /// Original file name
    pub file_name: String,
    /// MIME type of the document
    pub mime_type: String,
    /// OCR provider to use (tesseract, aws_textract, google_vision)
    pub provider: String,
    /// Current job status
    pub status: OcrJobStatus,
    /// Number of processing attempts
    pub attempt_count: i32,
    /// Maximum allowed attempts
    pub max_attempts: i32,
    /// OCR extraction result (JSON, populated on completion)
    pub result: Option<serde_json::Value>,
    /// Matched vendor ID (populated after vendor matching)
    pub matched_vendor_id: Option<Uuid>,
    /// Vendor match confidence score (0.0 to 1.0)
    pub vendor_match_confidence: Option<f32>,
    /// Error message (populated on failure)
    pub error_message: Option<String>,
    /// Processing duration in milliseconds
    pub processing_time_ms: Option<i64>,
    /// Priority (lower = higher priority)
    pub priority: i32,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
    /// Started processing timestamp
    pub started_at: Option<DateTime<Utc>>,
    /// Completed timestamp
    pub completed_at: Option<DateTime<Utc>>,
}

// ──────────────────────────── Input Types ────────────────────────────

/// Input for creating an OCR job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOcrJobInput {
    /// Document file ID in storage
    pub document_id: Uuid,
    /// Original file name
    pub file_name: String,
    /// MIME type of the document
    pub mime_type: String,
    /// OCR provider to use (defaults to "tesseract")
    pub provider: Option<String>,
    /// Priority (lower = higher priority, defaults to 100)
    pub priority: Option<i32>,
    /// Maximum retry attempts (defaults to 3)
    pub max_attempts: Option<i32>,
}

// ──────────────────────────── Batch Types ────────────────────────────

/// Result of a batch upload operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchUploadResult {
    /// Total number of files submitted
    pub total_files: usize,
    /// Number of jobs successfully created
    pub jobs_created: usize,
    /// Job IDs for successfully created jobs
    pub job_ids: Vec<Uuid>,
    /// Errors for files that failed
    pub errors: Vec<BatchError>,
}

/// Error for a single file in a batch upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchError {
    /// File name that failed
    pub file_name: String,
    /// Error message
    pub error: String,
}

// ──────────────────────────── Corrections ────────────────────────────

/// A user correction to an OCR-extracted field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrFieldCorrection {
    /// Job ID the correction applies to
    pub job_id: Uuid,
    /// Field name that was corrected (e.g., "vendor_name", "invoice_number")
    pub field_name: String,
    /// Original OCR-extracted value
    pub original_value: Option<String>,
    /// User-corrected value
    pub corrected_value: String,
    /// User who made the correction
    pub corrected_by: Uuid,
    /// When the correction was made
    pub corrected_at: DateTime<Utc>,
}

// ──────────────────────────── Vendor Matching ────────────────────────────

/// A vendor alias for matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorAlias {
    /// Alias ID
    pub id: Uuid,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Vendor ID this alias maps to
    pub vendor_id: Uuid,
    /// The alias string (e.g., "ACME Corp" → maps to vendor "Acme Corporation")
    pub alias: String,
    /// Whether this alias was learned automatically from corrections
    pub is_learned: bool,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

/// Result of a vendor match attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorMatchResult {
    /// Matched vendor ID (None if no match found)
    pub vendor_id: Option<Uuid>,
    /// Matched vendor name
    pub vendor_name: Option<String>,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f32,
    /// How the match was determined
    pub match_method: VendorMatchMethod,
}

/// Method used to match a vendor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VendorMatchMethod {
    /// Exact name match
    Exact,
    /// Matched via known alias
    Alias,
    /// Fuzzy string similarity match
    Fuzzy,
    /// No match found
    None,
}

impl std::fmt::Display for VendorMatchMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact => write!(f, "exact"),
            Self::Alias => write!(f, "alias"),
            Self::Fuzzy => write!(f, "fuzzy"),
            Self::None => write!(f, "none"),
        }
    }
}

// ──────────────────────────── Statistics ────────────────────────────

/// Processing statistics for the OCR pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrProcessingStats {
    /// Total jobs created
    pub total_jobs: i64,
    /// Jobs currently pending
    pub pending_jobs: i64,
    /// Jobs currently processing
    pub processing_jobs: i64,
    /// Jobs completed successfully
    pub completed_jobs: i64,
    /// Jobs that failed
    pub failed_jobs: i64,
    /// Jobs cancelled
    pub cancelled_jobs: i64,
    /// Average processing time in milliseconds
    pub avg_processing_time_ms: Option<f64>,
    /// Total corrections recorded
    pub total_corrections: i64,
    /// Vendor match rate (0.0 to 1.0)
    pub vendor_match_rate: Option<f64>,
}
