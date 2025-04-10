use std::{any::Any, collections::HashMap, fmt, sync::Arc};

use arrow_schema::SchemaRef;
use async_trait::async_trait;
use datafusion::{
    arrow::array::Array,
    catalog::Session,
    common::{SchemaExt, not_impl_err},
    datasource::{TableProvider, TableType},
    error::{DataFusionError, Result as DFResult},
    execution::{TaskContext, context::SessionContext},
    logical_expr::{BinaryExpr, Expr, Operator, TableProviderFilterPushDown, dml::InsertOp},
    physical_plan::{
        DisplayAs, DisplayFormatType, ExecutionPlan, SendableRecordBatchStream,
        insert::{DataSink, DataSinkExec},
    },
    scalar::ScalarValue,
};
use delta_kernel::arrow::record_batch::RecordBatch;
use deltalake::{DeltaOps, DeltaTable, DeltaTableBuilder, storage::StorageOptions};
use futures::StreamExt;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use url::Url;

use crate::{
    config::Config,
    error::{Result, TimeFusionError},
    persistent_queue::OtelLogsAndSpans,
};

type ProjectConfig = (String, StorageOptions, Arc<RwLock<DeltaTable>>);

pub type ProjectConfigs = Arc<RwLock<HashMap<String, ProjectConfig>>>;

#[derive(Debug)]
pub struct Database {
    project_configs: ProjectConfigs,
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            project_configs: Arc::clone(&self.project_configs),
        }
    }
}

impl Database {
    #[tracing::instrument(name = "db.new", skip(config))]
    pub async fn new(config: &Config) -> Result<Self> {
        let storage_uri = format!("s3://{}/{}/?endpoint={}", config.s3_bucket, config.table_prefix, config.s3_endpoint);
        info!("Storage URI configured: {}", storage_uri);

        let aws_url = Url::parse(&config.s3_endpoint).map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Invalid AWS endpoint URL: {}", e)))?;
        deltalake::aws::register_handlers(Some(aws_url));
        info!("AWS handlers registered");

        let project_configs = HashMap::new();

        let db = Self {
            project_configs: Arc::new(RwLock::new(project_configs)),
        };

        // Pass credentials to register_project since they're required
        db.register_project(
            "default",
            &storage_uri,
            Some(&config.aws_access_key_id),
            Some(&config.aws_secret_access_key),
            Some(&config.s3_endpoint),
        )
        .await?;

        Ok(db)
    }

    #[cfg(test)]
    #[tracing::instrument(name = "db.new_for_test", skip(config))]
    pub async fn new_for_test(config: &Config) -> Result<Self> {
        info!("Starting Database in test mode");

        let storage_uri = format!("s3://{}/{}/?endpoint={}", config.s3_bucket, config.table_prefix, config.s3_endpoint);
        info!("Storage URI configured: {}", storage_uri);

        let aws_url = Url::parse(&config.s3_endpoint).map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Invalid AWS endpoint URL: {}", e)))?;
        deltalake::aws::register_handlers(Some(aws_url));
        info!("AWS handlers registered");

        let project_configs = HashMap::new();

        let db = Self {
            project_configs: Arc::new(RwLock::new(project_configs)),
        };

        info!("Registering project with explicit credentials");
        db.register_project(
            "default",
            &storage_uri,
            Some(&config.aws_access_key_id),
            Some(&config.aws_secret_access_key),
            Some(&config.s3_endpoint),
        )
        .await?;

        Ok(db)
    }

    #[tracing::instrument(name = "db.create_session_context", skip(self))]
    pub fn create_session_context(&self) -> SessionContext {
        use datafusion::config::ConfigOptions;

        let mut options = ConfigOptions::new();
        let _ = options.set("datafusion.sql_parser.enable_information_schema", "true");
        SessionContext::new_with_config(options.into())
    }

    #[tracing::instrument(name = "db.setup_session_context", skip(self, ctx))]
    pub fn setup_session_context(&self, ctx: &SessionContext) -> DFResult<()> {
        use crate::persistent_queue::OtelLogsAndSpans;

        let schema = OtelLogsAndSpans::schema_ref();
        let routing_table = ProjectRoutingTable::new("default".to_string(), Arc::new(self.clone()), schema);
        ctx.register_table(OtelLogsAndSpans::table_name(), Arc::new(routing_table))?;
        info!("Registered ProjectRoutingTable with SessionContext");

        self.register_pg_settings_table(ctx)?;
        self.register_set_config_udf(ctx);

        Ok(())
    }

    #[tracing::instrument(name = "db.register_pg_settings_table", skip(self, ctx))]
    pub fn register_pg_settings_table(&self, ctx: &SessionContext) -> DFResult<()> {
        use datafusion::arrow::{
            array::StringArray,
            datatypes::{DataType, Field, Schema},
            record_batch::RecordBatch,
        };

        let schema = Arc::new(Schema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("setting", DataType::Utf8, false),
        ]));

        let names = vec!["TimeZone".to_string(), "client_encoding".to_string(), "datestyle".to_string(), "client_min_messages".to_string()];
        let settings = vec!["UTC".to_string(), "UTF8".to_string(), "ISO, MDY".to_string(), "notice".to_string()];

        let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(StringArray::from(names)), Arc::new(StringArray::from(settings))])?;

        ctx.register_batch("pg_settings", batch)?;
        Ok(())
    }

    #[tracing::instrument(name = "db.register_set_config_udf", skip(self, ctx))]
    pub fn register_set_config_udf(&self, ctx: &SessionContext) {
        use datafusion::{
            arrow::{
                array::{StringArray, StringBuilder},
                datatypes::DataType,
            },
            logical_expr::{ColumnarValue, ScalarFunctionImplementation, Volatility, create_udf},
        };

        let set_config_fn: ScalarFunctionImplementation = Arc::new(move |args: &[ColumnarValue]| -> DFResult<ColumnarValue> {
            let param_value_array = match &args[1] {
                ColumnarValue::Array(array) => array.as_any().downcast_ref::<StringArray>().expect("set_config second arg must be a StringArray"),
                _ => panic!("set_config second arg must be an array"),
            };

            let mut builder = StringBuilder::new();
            for i in 0..param_value_array.len() {
                if param_value_array.is_null(i) {
                    builder.append_null();
                } else {
                    builder.append_value(param_value_array.value(i));
                }
            }
            Ok(ColumnarValue::Array(Arc::new(builder.finish())))
        });

        let set_config_udf = create_udf(
            "set_config",
            vec![DataType::Utf8, DataType::Utf8, DataType::Boolean],
            DataType::Utf8,
            Volatility::Volatile,
            set_config_fn,
        );

        ctx.register_udf(set_config_udf);
    }

    #[tracing::instrument(name = "db.start_pgwire_server", skip(self, session_context, shutdown_token), fields(port))]
    pub async fn start_pgwire_server(
        &self, session_context: SessionContext, port: u16, shutdown_token: CancellationToken,
    ) -> Result<tokio::task::JoinHandle<()>> {
        use datafusion_postgres::{DfSessionService, HandlerFactory};
        use tokio::net::TcpListener;

        let pg_service = Arc::new(DfSessionService::new(session_context));
        let handler_factory = Arc::new(HandlerFactory(pg_service.clone()));
        let pg_listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await.map_err(TimeFusionError::Io)?;
        info!("PGWire server running on 0.0.0.0:{}", port);

        let pgwire_shutdown = shutdown_token.clone();

        let pg_server = tokio::spawn({
            let handler_factory = handler_factory.clone();
            async move {
                loop {
                    tokio::select! {
                        _ = pgwire_shutdown.cancelled() => {
                            info!("PGWire server shutting down.");
                            break;
                        }
                        result = pg_listener.accept() => {
                            match result {
                                Ok((socket, addr)) => {
                                    debug!("PGWire: Received connection from {}, preparing to process", addr);
                                    let handler_factory = handler_factory.clone();
                                    tokio::spawn(async move {
                                        match pgwire::tokio::process_socket(socket, None, handler_factory).await {
                                            Ok(()) => {
                                                info!("PGWire: Connection from {} processed successfully", addr);
                                            }
                                            Err(e) => {
                                                error!("PGWire: Error processing connection from {}: {:?}", addr, e);
                                            }
                                        }
                                    });
                                }
                                Err(e) => {
                                    error!("PGWire: Error accepting connection: {:?}", e);
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(pg_server)
    }

    #[tracing::instrument(name = "db.resolve_table", skip(self), fields(project_id))]
    pub async fn resolve_table(&self, project_id: &str) -> DFResult<Arc<RwLock<DeltaTable>>> {
        let project_configs = self.project_configs.read().await;

        if let Some((_, _, table)) = project_configs.get(project_id) {
            return Ok(table.clone());
        }

        if project_id != "default" {
            if let Some((_, _, table)) = project_configs.get("default") {
                log::warn!("Project '{}' not found, falling back to default project", project_id);
                return Ok(table.clone());
            }
        }

        Err(DataFusionError::Execution(format!(
            "Unknown project_id: {} and no default project found",
            project_id
        )))
    }

    #[tracing::instrument(name = "db.insert_records_batch", skip(self, _table, batch), fields(batch_size = batch.len()))]
    pub async fn insert_records_batch(&self, _table: &str, batch: Vec<RecordBatch>) -> Result<()> {
        let (_conn_str, _options, table_ref) = {
            let configs = self.project_configs.read().await;
            configs
                .get("default")
                .ok_or_else(|| TimeFusionError::Generic(anyhow::anyhow!("Project ID 'default' not found")))?
                .clone()
        };

        let mut table = table_ref.write().await;
        let ops = DeltaOps(table.clone());

        let write_op = ops.write(batch).with_partition_columns(OtelLogsAndSpans::partitions());
        *table = write_op.await.map_err(TimeFusionError::Database)?;

        Ok(())
    }

    #[cfg(test)]
    #[tracing::instrument(name = "db.insert_records", skip(self, records))]
    pub async fn insert_records(&self, records: &Vec<crate::persistent_queue::OtelLogsAndSpans>) -> Result<()> {
        use serde_arrow::schema::SchemaLike;

        let fields = Vec::<arrow_schema::FieldRef>::from_type::<OtelLogsAndSpans>(serde_arrow::schema::TracingOptions::default())
            .map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Failed to create schema fields: {}", e)))?;
        let batch = serde_arrow::to_record_batch(&fields, &records)
            .map_err(|e| TimeFusionError::Generic(anyhow::anyhow!("Failed to convert to record batch: {}", e)))?;

        self.insert_records_batch("default", vec![batch]).await
    }

    #[tracing::instrument(name = "db.register_project", skip(self, conn_str, access_key, secret_key, endpoint), fields(project_id))]
    pub async fn register_project(
        &self, project_id: &str, conn_str: &str, access_key: Option<&str>, secret_key: Option<&str>, endpoint: Option<&str>,
    ) -> Result<()> {
        let mut storage_options = StorageOptions::default();

        if let Some(key) = access_key.filter(|k| !k.is_empty()) {
            storage_options.0.insert("AWS_ACCESS_KEY_ID".to_string(), key.to_string());
        }

        if let Some(key) = secret_key.filter(|k| !k.is_empty()) {
            storage_options.0.insert("AWS_SECRET_ACCESS_KEY".to_string(), key.to_string());
        }

        if let Some(ep) = endpoint.filter(|e| !e.is_empty()) {
            storage_options.0.insert("AWS_ENDPOINT".to_string(), ep.to_string());
        }

        storage_options.0.insert("AWS_ALLOW_HTTP".to_string(), "true".to_string());

        let table = match DeltaTableBuilder::from_uri(conn_str).with_storage_options(storage_options.0.clone()).with_allow_http(true).load().await {
            Ok(table) => table,
            Err(err) => {
                log::warn!("table doesn't exist. creating new table. err: {:?}", err);

                let delta_ops = DeltaOps::try_from_uri(&conn_str).await.map_err(TimeFusionError::Database)?;
                delta_ops
                    .create()
                    .with_columns(OtelLogsAndSpans::columns().unwrap_or_default())
                    .with_partition_columns(OtelLogsAndSpans::partitions())
                    .with_storage_options(storage_options.0.clone())
                    .await
                    .map_err(TimeFusionError::Database)?
            }
        };

        let mut configs = self.project_configs.write().await;
        configs.insert(project_id.to_string(), (conn_str.to_string(), storage_options, Arc::new(RwLock::new(table))));
        Ok(())
    }

    #[tracing::instrument(name = "db.flush_pending_writes", skip(self))]
    pub async fn flush_pending_writes(&self) -> Result<()> {
        let configs = self.project_configs.read().await;
        for (project_id, (_, _, table)) in configs.iter() {
            let mut table = table.write().await;
            *table = deltalake::open_table(&table.table_uri()).await.map_err(TimeFusionError::Database)?;
            debug!("Flushed pending writes for project: {}", project_id);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ProjectRoutingTable {
    default_project: String,
    database:        Arc<Database>,
    schema:          SchemaRef,
}

impl ProjectRoutingTable {
    pub fn new(default_project: String, database: Arc<Database>, schema: SchemaRef) -> Self {
        Self {
            default_project,
            database,
            schema,
        }
    }

    fn extract_project_id_from_filters(&self, filters: &[Expr]) -> Option<String> {
        for filter in filters {
            if let Some(project_id) = self.extract_project_id(filter) {
                return Some(project_id);
            }
        }
        None
    }

    fn schema(&self) -> SchemaRef {
        OtelLogsAndSpans::schema_ref()
    }

    fn extract_project_id(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::BinaryExpr(BinaryExpr { left, op, right }) => {
                if *op == Operator::Eq {
                    if let Expr::Column(col) = left.as_ref() {
                        if col.name == "project_id" {
                            if let Expr::Literal(ScalarValue::Utf8(Some(value))) = right.as_ref() {
                                return Some(value.clone());
                            }
                        }
                    }
                    if let Expr::Column(col) = right.as_ref() {
                        if col.name == "project_id" {
                            if let Expr::Literal(ScalarValue::Utf8(Some(value))) = left.as_ref() {
                                return Some(value.clone());
                            }
                        }
                    }
                }
                None
            }
            Expr::Not(inner) => self.extract_project_id(inner),
            _ => None,
        }
    }
}

impl DisplayAs for ProjectRoutingTable {
    fn fmt_as(&self, t: DisplayFormatType, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match t {
            DisplayFormatType::Default | DisplayFormatType::Verbose => {
                write!(f, "ProjectRoutingTable ")
            }
        }
    }
}

#[async_trait]
impl DataSink for ProjectRoutingTable {
    fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    async fn write_all(&self, mut data: SendableRecordBatchStream, _context: &Arc<TaskContext>) -> DFResult<u64> {
        let mut new_batches = vec![];
        let mut row_count = 0;
        while let Some(batch) = data.next().await.transpose()? {
            row_count += batch.num_rows();
            new_batches.push(batch);
        }
        self.database
            .insert_records_batch("", new_batches)
            .await
            .map_err(|e| DataFusionError::Execution(format!("Failed to insert records: {}", e)))?;
        Ok(row_count as u64)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[async_trait]
impl TableProvider for ProjectRoutingTable {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    fn schema(&self) -> SchemaRef {
        self.schema()
    }

    async fn insert_into(&self, _state: &dyn Session, input: Arc<dyn ExecutionPlan>, insert_op: InsertOp) -> DFResult<Arc<dyn ExecutionPlan>> {
        self.schema().logically_equivalent_names_and_types(&input.schema())?;

        if insert_op != InsertOp::Append {
            return not_impl_err!("{insert_op} not implemented for MemoryTable yet");
        }

        Ok(Arc::new(DataSinkExec::new(input, Arc::new(self.clone()), None)))
    }

    fn supports_filters_pushdown(&self, filter: &[&Expr]) -> DFResult<Vec<TableProviderFilterPushDown>> {
        Ok(filter.iter().map(|_| TableProviderFilterPushDown::Inexact).collect())
    }

    async fn scan(&self, state: &dyn Session, projection: Option<&Vec<usize>>, filters: &[Expr], limit: Option<usize>) -> DFResult<Arc<dyn ExecutionPlan>> {
        let project_id = self.extract_project_id_from_filters(filters).unwrap_or_else(|| self.default_project.clone());

        let delta_table = self.database.resolve_table(&project_id).await?;
        let table = delta_table.read().await;
        table.scan(state, projection, filters, limit).await
    }
}

#[cfg(test)]
mod tests {
    use std::thread::sleep;

    use chrono::{TimeZone, Utc};
    use datafusion::{assert_batches_eq, prelude::SessionContext};
    use dotenv::dotenv;
    use serial_test::serial;
    use tokio::time;
    use uuid::Uuid;

    use super::*;

    // Helper function to create a test database with a unique table prefix
    async fn setup_test_database(prefix: String) -> Result<(Database, SessionContext, String)> {
        let _ = env_logger::builder().is_test(true).try_init();
        dotenv().ok();

        // Set a unique test-specific prefix for a clean Delta table
        let test_prefix = format!("test-data-{}", prefix);
        unsafe {
            env::set_var("TIMEFUSION_TABLE_PREFIX", &test_prefix);
        }

        let db = Database::new().await?;
        let mut session_context = SessionContext::new();
        datafusion_functions_json::register_all(&session_context)?;
        let schema = OtelLogsAndSpans::schema_ref();

        let routing_table = ProjectRoutingTable::new("default".to_string(), Arc::new(db.clone()), schema);
        session_context.register_table(OtelLogsAndSpans::table_name(), Arc::new(routing_table))?;

        Ok((db, session_context, test_prefix))
    }

    // Helper function to refresh the table after modifications
    async fn refresh_table(db: &Database, session_context: &SessionContext) -> Result<()> {
        // Refresh the Delta table
        // Get the delta table using our resolve_table method
        let table_ref = db.resolve_table("default").await.map_err(|e| anyhow::anyhow!("Failed to resolve default table: {:?}", e))?;

        let mut table = table_ref.write().await;
        *table = deltalake::open_table(&table.table_uri()).await?;

        // Create a new ProjectRoutingTable with the refreshed database
        let schema = OtelLogsAndSpans::schema_ref();
        let routing_table = ProjectRoutingTable::new("default".to_string(), Arc::new(db.clone()), schema);

        // Re-register the table with the session context
        session_context.deregister_table(OtelLogsAndSpans::table_name())?;
        session_context.register_table(OtelLogsAndSpans::table_name(), Arc::new(routing_table))?;

        // Add a small delay to ensure the changes propagate
        sleep(time::Duration::from_millis(100));

        Ok(())
    }

    // Helper function to create sample test records
    fn create_test_records() -> Vec<OtelLogsAndSpans> {
        let timestamp1 = Utc.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap();
        let timestamp2 = Utc.with_ymd_and_hms(2023, 1, 1, 10, 10, 0).unwrap();

        vec![
            OtelLogsAndSpans {
                project_id: "test_project".to_string(),
                timestamp: timestamp1,
                observed_timestamp: Some(timestamp1),
                id: "span1".to_string(),
                name: Some("test_span_1".to_string()),
                context___trace_id: Some("trace1".to_string()),
                context___span_id: Some("span1".to_string()),
                start_time: Some(timestamp1),
                duration: Some(100_000_000),
                status_code: Some("OK".to_string()),
                level: Some("INFO".to_string()),
                ..Default::default()
            },
            OtelLogsAndSpans {
                project_id: "test_project".to_string(),
                timestamp: timestamp2,
                observed_timestamp: Some(timestamp2),
                id: "span2".to_string(),
                name: Some("test_span_2".to_string()),
                context___trace_id: Some("trace2".to_string()),
                context___span_id: Some("span2".to_string()),
                start_time: Some(timestamp2),
                duration: Some(200_000_000),
                status_code: Some("ERROR".to_string()),
                level: Some("ERROR".to_string()),
                status_message: Some("Error occurred".to_string()),
                ..Default::default()
            },
        ]
    }

    #[serial]
    #[tokio::test]
    async fn test_database_query() -> Result<()> {
        let (db, ctx, test_prefix) = setup_test_database(Uuid::new_v4().to_string() + "query").await?;
        log::info!("Using test-specific table prefix: {}", test_prefix);

        let records = create_test_records();
        db.insert_records(&records).await?;

        // Test 1: Basic count query to verify record insertion
        let count_df = ctx.sql("SELECT COUNT(*) as count FROM otel_logs_and_spans").await?;
        let result = count_df.collect().await?;

        #[rustfmt::skip]
        assert_batches_eq!(
            [
                "+-------+", 
                "| count |", 
                "+-------+", 
                "| 2     |", 
                "+-------+",
            ], &result);

        // Test 2: Query with field selection and ordering
        log::info!("Testing field selection and ordering");
        let df = ctx.sql("SELECT timestamp, name, status_code, level FROM otel_logs_and_spans ORDER BY name").await?;
        let result = df.collect().await?;

        assert_batches_eq!(
            [
                "+---------------------+-------------+-------------+-------+",
                "| timestamp           | name        | status_code | level |",
                "+---------------------+-------------+-------------+-------+",
                "| 2023-01-01T10:00:00 | test_span_1 | OK          | INFO  |",
                "| 2023-01-01T10:10:00 | test_span_2 | ERROR       | ERROR |",
                "+---------------------+-------------+-------------+-------+",
            ],
            &result
        );

        // Test 3: Filtering by project_id and level
        log::info!("Testing filtering by project_id and level");
        let df = ctx
            .sql("SELECT name, level, status_code, status_message FROM otel_logs_and_spans WHERE project_id = 'test_project' AND level = 'ERROR'")
            .await?;
        let result = df.collect().await?;

        assert_batches_eq!(
            [
                "+-------------+-------+-------------+----------------+",
                "| name        | level | status_code | status_message |",
                "+-------------+-------+-------------+----------------+",
                "| test_span_2 | ERROR | ERROR       | Error occurred |",
                "+-------------+-------+-------------+----------------+",
            ],
            &result
        );

        // Test 4: Complex query with multiple data types (including timestamp and end_time)
        // Note: For timestamp columns, we need to format them for the test to use assert_batches_eq
        log::info!("Testing complex query with multiple data types");
        let df = ctx
            .sql(
                "
                    SELECT
                        id,
                        name,
                        project_id,
                        duration,
                        CAST(duration / 1000000 AS INTEGER) as duration_ms,
                        context___trace_id,
                        status_code,
                        level,
                        to_char(timestamp, '%Y-%m-%d %H:%M') as formatted_timestamp,
                        to_char(end_time, 'YYYY-MM-DD HH24:MI') as formatted_end_time,
                        CASE WHEN status_code = 'ERROR' THEN status_message ELSE NULL END as conditional_message
                    FROM otel_logs_and_spans
                    ORDER BY id
                ",
            )
            .await?;
        let result = df.collect().await?;

        assert_batches_eq!(
            [
                "+-------+-------------+--------------+-----------+-------------+--------------------+-------------+-------+---------------------+--------------------+---------------------+",
                "| id    | name        | project_id   | duration  | duration_ms | context___trace_id | status_code | level | formatted_timestamp | formatted_end_time | conditional_message |",
                "+-------+-------------+--------------+-----------+-------------+--------------------+-------------+-------+---------------------+--------------------+---------------------+",
                "| span1 | test_span_1 | test_project | 100000000 | 100         | trace1             | OK          | INFO  | 2023-01-01 10:00    |                    |                     |",
                "| span2 | test_span_2 | test_project | 200000000 | 200         | trace2             | ERROR       | ERROR | 2023-01-01 10:10    |                    | Error occurred      |",
                "+-------+-------------+--------------+-----------+-------------+--------------------+-------------+-------+---------------------+--------------------+---------------------+",
            ],
            &result
        );

        // Test 5: Timestamp filtering
        log::info!("Testing timestamp filtering");
        let df = ctx.sql("SELECT COUNT(*) as count FROM otel_logs_and_spans WHERE timestamp > '2023-01-01T10:05:00.000000Z'").await?;
        let result = df.collect().await?;

        #[rustfmt::skip]
            assert_batches_eq!(
                [
                    "+-------+",
                    "| count |",
                    "+-------+",
                    "| 1     |",
                    "+-------+",
                ],
                &result
            );

        // Test 6: Duration filtering
        log::info!("Testing duration filtering");
        let df = ctx.sql("SELECT name FROM otel_logs_and_spans WHERE duration > 150000000").await?;
        let result = df.collect().await?;

        #[rustfmt::skip]
            assert_batches_eq!(
                [
                    "+-------------+",
                    "| name        |",
                    "+-------------+",
                    "| test_span_2 |",
                    "+-------------+",
                ],
                &result
            );

        // Test 7: Complex filtering with timestamp columns in ISO 8601 format
        log::info!("Testing complex filtering with timestamp columns in ISO 8601 format");
        let df = ctx
            .sql(
                "
                    WITH time_data AS (
                        SELECT
                            name,
                            status_code,
                            level,
                            CAST(duration / 1000000 AS INTEGER) as duration_ms,
                            timestamp,
                            to_char(timestamp, '%Y-%m-%d') as date_only,
                            EXTRACT(HOUR FROM timestamp) as hour,
                            to_char(timestamp, '%Y-%m-%dT%H:%M:%S%.6fZ') as iso_timestamp
                        FROM otel_logs_and_spans
                        WHERE
                            project_id = 'test_project'
                            AND timestamp > '2023-01-01T10:05:00.000000Z'
                    )
                    SELECT
                        name,
                        status_code,
                        level,
                        duration_ms,
                        date_only,
                        hour,
                        iso_timestamp
                    FROM time_data
                    ORDER BY name
                ",
            )
            .await?;
        let result = df.collect().await?;

        assert_batches_eq!(
            [
                "+-------------+-------------+-------+-------------+------------+------+-----------------------------+",
                "| name        | status_code | level | duration_ms | date_only  | hour | iso_timestamp               |",
                "+-------------+-------------+-------+-------------+------------+------+-----------------------------+",
                "| test_span_2 | ERROR       | ERROR | 200         | 2023-01-01 | 10   | 2023-01-01T10:10:00.000000Z |",
                "+-------------+-------------+-------+-------------+------------+------+-----------------------------+",
            ],
            &result
        );

        Ok(())
    }

    #[serial]
    #[tokio::test]
    async fn test_sql_insert() -> Result<()> {
        let (db, ctx, test_prefix) = setup_test_database(Uuid::new_v4().to_string() + "insert").await?;
        log::info!("Using test-specific table prefix for SQL INSERT test: {}", test_prefix);

        let datetime = chrono::DateTime::parse_from_rfc3339("2023-02-01T15:30:00.000000Z").unwrap().with_timezone(&chrono::Utc);
        let record = OtelLogsAndSpans {
            project_id: "default".to_string(),
            timestamp: datetime,
            observed_timestamp: Some(datetime),
            id: "sql_span1a".to_string(),
            name: Some("sql_test_span".to_string()),
            duration: Some(150000000),
            start_time: Some(datetime),
            context___trace_id: Some("sql_trace1".to_string()),
            context___span_id: Some("sql_span1".to_string()),
            status_code: Some("OK".to_string()),
            status_message: Some("SQL inserted successfully".to_string()),
            level: Some("INFO".to_string()),
            ..Default::default()
        };
        db.insert_records(&vec![record]).await?;

        let verify_df = ctx.sql("SELECT id, name, timestamp from otel_logs_and_spans").await?.collect().await?;
        #[rustfmt::skip]
        assert_batches_eq!(
            [
                "+------------+---------------+---------------------+",
                "| id         | name          | timestamp           |",
                "+------------+---------------+---------------------+",
                "| sql_span1a | sql_test_span | 2023-02-01T15:30:00 |",
                "+------------+---------------+---------------------+",
        ], &verify_df);
        //
        let insert_sql = "INSERT INTO otel_logs_and_spans (
                project_id, timestamp, id,
                parent_id, name, kind,
                status_code, status_message, level, severity___severity_text, severity___severity_number,
                body, duration, start_time, end_time
            ) VALUES (
                'test_project', TIMESTAMP '2023-01-01T10:00:00Z', 'sql_span1',
                NULL, 'sql_test_span', NULL,
                'OK', 'span inserted successfully', 'INFO', 'INFORMATION', NULL,
                NULL, 150000000, TIMESTAMP '2023-01-01T10:00:00Z', NULL
            )";

        let insert_result = ctx.sql(insert_sql).await?.collect().await?;
        #[rustfmt::skip]
        assert_batches_eq!(
            ["+-------+",
            "| count |",
            "+-------+",
            "| 1     |",
            "+-------+",
        ], &insert_result);

        let verify_df = ctx
            .sql("SELECT project_id, id, name, timestamp, kind, status_code, severity___severity_text, duration, start_time from otel_logs_and_spans order by timestamp desc")
            .await?
            .collect()
            .await?;
        #[rustfmt::skip]
        assert_batches_eq!(
        [
            "+--------------+------------+---------------+---------------------+------+-------------+--------------------------+-----------+---------------------+",
            "| project_id   | id         | name          | timestamp           | kind | status_code | severity___severity_text | duration  | start_time          |",
            "+--------------+------------+---------------+---------------------+------+-------------+--------------------------+-----------+---------------------+",
            "| default      | sql_span1a | sql_test_span | 2023-02-01T15:30:00 |      | OK          |                          | 150000000 | 2023-02-01T15:30:00 |",
            "| test_project | sql_span1  | sql_test_span | 2023-01-01T10:00:00 |      | OK          | INFORMATION              | 150000000 | 2023-01-01T10:00:00 |",
            "+--------------+------------+---------------+---------------------+------+-------------+--------------------------+-----------+---------------------+",
        ]
        , &verify_df);

        log::info!("Inserting record directly via insert statement");
        let insert_sql = "INSERT INTO otel_logs_and_spans (
                project_id, timestamp, observed_timestamp, id,
                parent_id, name, kind,
                status_code, status_message, level, severity___severity_text, severity___severity_number,
                body, duration, start_time, end_time,
                context___trace_id, context___span_id, context___trace_state, context___trace_flags,
                context___is_remote, events, links,
                attributes___client___address, attributes___client___port,

                attributes___server___address, attributes___server___port, attributes___network___local__address, attributes___network___local__port,
                attributes___network___peer___address, attributes___network___peer__port, attributes___network___protocol___name, attributes___network___protocol___version,
                attributes___network___transport, attributes___network___type,attributes___code___number, attributes___code___file___path,
                attributes___code___function___name, attributes___code___line___number, attributes___code___stacktrace, attributes___log__record___original,

                attributes___log__record___uid, attributes___error___type, attributes___exception___type, attributes___exception___message,
                attributes___exception___stacktrace, attributes___url___fragment, attributes___url___full, attributes___url___path,
                attributes___url___query, attributes___url___scheme, attributes___user_agent___original, attributes___http___request___method,
                attributes___http___request___method_original,attributes___http___response___status_code, attributes___http___request___resend_count, attributes___http___request___body___size,

                attributes___session___id, attributes___session___previous___id, attributes___db___system___name, attributes___db___collection___name,
                attributes___db___namespace, attributes___db___operation___name, attributes___db___response___status_code, attributes___db___operation___batch___size,
                attributes___db___query___summary, attributes___db___query___text, attributes___user___id, attributes___user___email,
                attributes___user___full_name, attributes___user___name, attributes___user___hash, resource___attributes___service___name,

                resource___attributes___service___version, resource___attributes___service___instance___id, resource___attributes___service___namespace, resource___attributes___telemetry___sdk___language,
                resource___attributes___telemetry___sdk___name, resource___attributes___telemetry___sdk___version, resource___attributes___user_agent___original
            ) VALUES (
                'test_project', TIMESTAMP '2023-01-02T10:00:00Z', NULL, 'sql_span2',
                NULL, 'sql_test_span', NULL,
                'OK', 'span inserted successfully', 'INFO', NULL, NULL,
                NULL, 150000000, TIMESTAMP '2023-01-01T10:00:00Z', NULL,
                'sql_trace1', 'sql_span1', NULL, NULL,
                NULL, NULL, NULL,
                NULL, NULL,

                NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL,

                NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL,

                NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL,

                NULL, NULL, NULL, NULL,
                NULL, NULL, NULL
            )";

        let insert_result = ctx.sql(insert_sql).await?.collect().await?;
        #[rustfmt::skip]
        assert_batches_eq!(
            ["+-------+",
            "| count |",
            "+-------+",
            "| 1     |",
            "+-------+",
        ], &insert_result);

        // Verify that the SQL-inserted record exists
        let verify_df = ctx.sql("SELECT id, name, status_message FROM otel_logs_and_spans WHERE id = 'sql_span1'").await?;
        let verify_result = verify_df.collect().await?;

        // Check that we can retrieve the inserted record
        assert_batches_eq!(
            [
                "+-----------+---------------+----------------------------+",
                "| id        | name          | status_message             |",
                "+-----------+---------------+----------------------------+",
                "| sql_span1 | sql_test_span | span inserted successfully |",
                "+-----------+---------------+----------------------------+",
            ],
            &verify_result
        );

        // TODO: verify the correct copy to syntax
        // let copy_sql = "COPY (VALUES (
        //         NULL, 'sql_span2copy',
        //         NULL, 'sql_test_span_copy', NULL,
        //         'OK', 'span copied into successfully', 'INFO', NULL, NULL,
        //         NULL, 150000000, TIMESTAMP '2023-01-01T10:00:00Z', NULL,
        //         'sql_trace1copy', 'sql_span1copy', NULL, NULL,
        //         NULL, NULL, NULL,
        //         NULL, NULL,
        //
        //         NULL, NULL, NULL, NULL,
        //         NULL, NULL, NULL, NULL,
        //         NULL, NULL, NULL, NULL,
        //         NULL, NULL, NULL, NULL,
        //
        //         NULL, NULL, NULL, NULL,
        //         NULL, NULL, NULL, NULL,
        //         NULL, NULL, NULL, NULL,
        //         NULL, NULL, NULL, NULL,
        //
        //         NULL, NULL, NULL, NULL,
        //         NULL, NULL, NULL, NULL,
        //         NULL, NULL, NULL, NULL,
        //         NULL, NULL, NULL, NULL,
        //
        //         NULL, NULL, NULL, NULL,
        //         NULL, NULL, NULL,
        //
        //         'test_project', TIMESTAMP '2023-01-02T10:00:00Z'
        //     )) TO otel_logs_and_spans ";
        //
        // let insert_result = ctx.sql(copy_sql).await?.collect().await?;
        // #[rustfmt::skip]
        // assert_batches_eq!(
        //     ["+-------+",
        //     "| count |",
        //     "+-------+",
        //     "| 1     |",
        //     "+-------+",
        // ], &insert_result);

        let verify_df = ctx
            .sql("SELECT project_id, id, name, timestamp, kind, status_code, severity___severity_text, duration, start_time from otel_logs_and_spans order by timestamp desc")
            .await?
            .collect()
            .await?;
        #[rustfmt::skip]
        assert_batches_eq!(
            [
                "+--------------+------------+---------------+---------------------+------+-------------+--------------------------+-----------+---------------------+",
                "| project_id   | id         | name          | timestamp           | kind | status_code | severity___severity_text | duration  | start_time          |",
                "+--------------+------------+---------------+---------------------+------+-------------+--------------------------+-----------+---------------------+",
                "| default      | sql_span1a | sql_test_span | 2023-02-01T15:30:00 |      | OK          |                          | 150000000 | 2023-02-01T15:30:00 |",
                "| test_project | sql_span2  | sql_test_span | 2023-01-02T10:00:00 |      | OK          |                          | 150000000 | 2023-01-01T10:00:00 |",
                "| test_project | sql_span1  | sql_test_span | 2023-01-01T10:00:00 |      | OK          | INFORMATION              | 150000000 | 2023-01-01T10:00:00 |",
                "+--------------+------------+---------------+---------------------+------+-------------+--------------------------+-----------+---------------------+",
            ],
            &verify_df
        );

        Ok(())
    }
}
