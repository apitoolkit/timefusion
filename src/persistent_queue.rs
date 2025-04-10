use std::sync::Arc;

use arrow_schema::{DataType, Field, Schema, SchemaRef, TimeUnit};
use delta_kernel::schema::StructField;
use serde::{Deserialize, Serialize};
use serde_arrow::schema::{SchemaLike, TracingOptions};
use serde_json::json;

use crate::error::{Result, TimeFusionError};

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct OtelLogsAndSpans {
    #[serde(with = "chrono::serde::ts_microseconds_option")]
    pub observed_timestamp: Option<chrono::DateTime<chrono::Utc>>,

    pub id:             String,
    pub parent_id:      Option<String>,
    pub name:           Option<String>,
    pub kind:           Option<String>,
    pub status_code:    Option<String>,
    pub status_message: Option<String>,

    // Logs specific
    pub level:                      Option<String>, // same as severity text
    pub severity___severity_text:   Option<String>,
    pub severity___severity_number: Option<String>,
    pub body:                       Option<String>, // body as json

    pub duration: Option<u64>, // nanoseconds

    #[serde(with = "chrono::serde::ts_microseconds_option")]
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(with = "chrono::serde::ts_microseconds_option")]
    pub end_time:   Option<chrono::DateTime<chrono::Utc>>,

    // Context
    pub context___trace_id:    Option<String>,
    pub context___span_id:     Option<String>,
    pub context___trace_state: Option<String>,
    pub context___trace_flags: Option<String>,
    pub context___is_remote:   Option<String>,

    // Events
    pub events: Option<String>, // events json

    // Links
    pub links: Option<String>, // links json

    // Attributes

    // Server and client
    pub attributes___client___address: Option<String>,
    pub attributes___client___port:    Option<u32>,
    pub attributes___server___address: Option<String>,
    pub attributes___server___port:    Option<u32>,

    // network https://opentelemetry.io/docs/specs/semconv/attributes-registry/network/
    pub attributes___network___local__address:     Option<String>,
    pub attributes___network___local__port:        Option<u32>,
    pub attributes___network___peer___address:     Option<String>,
    pub attributes___network___peer__port:         Option<u32>,
    pub attributes___network___protocol___name:    Option<String>,
    pub attributes___network___protocol___version: Option<String>,
    pub attributes___network___transport:          Option<String>,
    pub attributes___network___type:               Option<String>,

    // Source Code Attributes
    pub attributes___code___number:          Option<u32>,
    pub attributes___code___file___path:     Option<u32>,
    pub attributes___code___function___name: Option<u32>,
    pub attributes___code___line___number:   Option<u32>,
    pub attributes___code___stacktrace:      Option<u32>,
    // Log records. https://opentelemetry.io/docs/specs/semconv/general/logs/
    pub attributes___log__record___original: Option<String>,
    pub attributes___log__record___uid:      Option<String>,

    // Exception https://opentelemetry.io/docs/specs/semconv/exceptions/exceptions-logs/
    pub attributes___error___type:           Option<String>,
    pub attributes___exception___type:       Option<String>,
    pub attributes___exception___message:    Option<String>,
    pub attributes___exception___stacktrace: Option<String>,

    // URL https://opentelemetry.io/docs/specs/semconv/attributes-registry/url/
    pub attributes___url___fragment: Option<String>,
    pub attributes___url___full:     Option<String>,
    pub attributes___url___path:     Option<String>,
    pub attributes___url___query:    Option<String>,
    pub attributes___url___scheme:   Option<String>,

    // Useragent https://opentelemetry.io/docs/specs/semconv/attributes-registry/user-agent/
    pub attributes___user_agent___original: Option<String>,

    // HTTP https://opentelemetry.io/docs/specs/semconv/http/http-spans/
    pub attributes___http___request___method:          Option<String>,
    pub attributes___http___request___method_original: Option<String>,
    pub attributes___http___response___status_code:    Option<String>,
    pub attributes___http___request___resend_count:    Option<String>,
    pub attributes___http___request___body___size:     Option<String>,

    // Session https://opentelemetry.io/docs/specs/semconv/general/session/
    pub attributes___session___id:            Option<String>,
    pub attributes___session___previous___id: Option<String>,

    // Database https://opentelemetry.io/docs/specs/semconv/database/database-spans/
    pub attributes___db___system___name:            Option<String>,
    pub attributes___db___collection___name:        Option<String>,
    pub attributes___db___namespace:                Option<String>,
    pub attributes___db___operation___name:         Option<String>,
    pub attributes___db___response___status_code:   Option<String>,
    pub attributes___db___operation___batch___size: Option<u32>,
    pub attributes___db___query___summary:          Option<String>,
    pub attributes___db___query___text:             Option<String>,

    // https://opentelemetry.io/docs/specs/semconv/attributes-registry/user/
    pub attributes___user___id:        Option<String>,
    pub attributes___user___email:     Option<String>,
    pub attributes___user___full_name: Option<String>,
    pub attributes___user___name:      Option<String>,
    pub attributes___user___hash:      Option<String>,

    // Resource Attributes (subset) https://opentelemetry.io/docs/specs/semconv/resource/
    pub resource___attributes___service___name:          Option<String>,
    pub resource___attributes___service___version:       Option<String>,
    pub resource___attributes___service___instance___id: Option<String>,
    pub resource___attributes___service___namespace:     Option<String>,

    pub resource___attributes___telemetry___sdk___language: Option<String>,
    pub resource___attributes___telemetry___sdk___name:     Option<String>,
    pub resource___attributes___telemetry___sdk___version:  Option<String>,

    pub resource___attributes___user_agent___original: Option<String>,
    // Kept at the bottom to make delta-rs happy, so its schema matches datafusion.
    // Seems delta removes the partition ids from the normal schema and moves them to the end.
    // Top-level fields
    pub project_id:                                    String,

    #[serde(with = "chrono::serde::ts_microseconds")]
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl OtelLogsAndSpans {
    pub fn table_name() -> String {
        "otel_logs_and_spans".to_string()
    }

    pub fn columns() -> Result<Vec<StructField>> {
        // Use custom Result
        let tracing_options = TracingOptions::default()
            .overwrite("project_id", json!({"name": "project_id", "data_type": "Utf8", "nullable": false}))
            .map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Failed to overwrite project_id: {}", e)))?
            .overwrite(
                "timestamp",
                json!({"name": "timestamp", "data_type": "Timestamp(Microsecond, None)", "nullable": false}),
            )
            .map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Failed to overwrite timestamp: {}", e)))?
            .overwrite("id", json!({"name": "id", "data_type": "Utf8", "nullable": false}))
            .map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Failed to overwrite id: {}", e)))?
            .overwrite(
                "observed_timestamp",
                json!({"name": "observed_timestamp", "data_type": "Timestamp(Microsecond, None)", "nullable": true}),
            )
            .map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Failed to overwrite observed_timestamp: {}", e)))?
            .overwrite(
                "start_time",
                json!({"name": "start_time", "data_type": "Timestamp(Microsecond, None)", "nullable": true}),
            )
            .map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Failed to overwrite start_time: {}", e)))?
            .overwrite(
                "end_time",
                json!({"name": "end_time", "data_type": "Timestamp(Microsecond, None)", "nullable": true}),
            )
            .map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Failed to overwrite end_time: {}", e)))?;

        let fields = Vec::<arrow_schema::FieldRef>::from_type::<OtelLogsAndSpans>(tracing_options)
            .map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Failed to generate fields: {}", e)))?;
        let vec_refs: Vec<StructField> = fields
            .iter()
            .map(|arc_field| arc_field.as_ref().try_into())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Failed to convert fields to StructField: {}", e)))?;

        // Validate the last two fields as a sanity check
        if fields.len() < 2
            || fields[fields.len() - 2].data_type() != &DataType::Utf8
            || fields[fields.len() - 1].data_type() != &DataType::Timestamp(TimeUnit::Microsecond, None)
        {
            return Err(TimeFusionError::Generic(anyhow::anyhow!(
                "Schema validation failed: expected project_id (Utf8) and timestamp (Timestamp) at end"
            )));
        }

        Ok(vec_refs)
    }

    pub fn schema_ref() -> SchemaRef {
        let columns = OtelLogsAndSpans::columns().unwrap_or_else(|e| {
            log::error!("Failed to get columns: {:?}", e);
            Vec::new()
        });

        let arrow_fields: Vec<Field> = columns.iter().filter_map(|sf| sf.try_into().ok()).collect();

        Arc::new(Schema::new(arrow_fields))
    }

    pub fn partitions() -> Vec<String> {
        vec!["project_id".to_string(), "timestamp".to_string()]
    }
}
