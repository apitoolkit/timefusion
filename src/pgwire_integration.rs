// src/pgwire_integration.rs

use async_trait::async_trait;
use pgwire::api::copy::NoopCopyHandler;
use pgwire::api::results::{
    DescribePortalResponse, DescribeStatementResponse, QueryResponse, Response, FieldInfo,
};
use pgwire::api::stmt::{QueryParser, StoredStatement};
use pgwire::api::{ClientInfo, Type, PgWireServerHandlers, NoopErrorHandler};
use pgwire::api::auth::StartupHandler;
use pgwire::messages::{PgWireFrontendMessage, PgWireBackendMessage};
use pgwire::messages::response::{ReadyForQuery, TransactionStatus};
use pgwire::messages::startup::Authentication;
use pgwire::error::{PgWireError, PgWireResult};
use pgwire::messages::data::DataRow;
use futures::SinkExt;
use tokio::sync::Mutex;
use std::sync::Arc;
use datafusion::prelude::*;
use datafusion::logical_expr::LogicalPlan;
use std::collections::HashMap;
use datafusion::common::ParamValues;
use bytes::BytesMut;
use crate::utils::{prepare_sql, value_to_string};
use tokio_util::sync::CancellationToken;
use tracing::{info, error, debug};
use std::fs;
use std::io::{Error as IoError, ErrorKind};
use serde::{Serialize, Deserialize};
use bcrypt::{hash, verify, DEFAULT_COST};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::datasource::MemTable;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub username: String,
    pub hashed_password: String,
    pub is_admin: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UserDB {
    pub users: Vec<User>,
}

impl UserDB {
    pub fn load_from_file(path: &str) -> Result<Self, IoError> {
        if let Ok(contents) = fs::read_to_string(path) {
            let db: UserDB = serde_json::from_str(&contents)
                .map_err(|e| IoError::new(ErrorKind::InvalidData, e))?;
            info!("Loaded user database from {}: {:?}", path, db.users);
            Ok(db)
        } else {
            info!("No user database found at {}. Creating default admin user.", path);
            let default_user = User {
                username: "admin".to_string(),
                hashed_password: hash("admin123", DEFAULT_COST)
                    .map_err(|e| IoError::new(ErrorKind::Other, e))?,
                is_admin: true,
            };
            let db = UserDB { users: vec![default_user] };
            db.save_to_file(path)?;
            Ok(db)
        }
    }

    pub fn save_to_file(&self, path: &str) -> Result<(), IoError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| IoError::new(ErrorKind::Other, e))?;
        fs::write(path, json)?;
        info!("Saved user database to {}", path);
        Ok(())
    }

    pub fn verify_user(&self, username: &str, password: &str) -> bool {
        if let Some(user) = self.users.iter().find(|u| u.username == username) {
            let result = verify(password, &user.hashed_password).unwrap_or(false);
            debug!("Verifying user '{}': password match = {}", username, result);
            result
        } else {
            debug!("User '{}' not found in database", username);
            false
        }
    }

    pub fn create_user(&mut self, username: &str, password: &str, is_admin: bool) -> Result<(), IoError> {
        if self.users.iter().any(|u| u.username == username) {
            return Err(IoError::new(ErrorKind::AlreadyExists, "User already exists"));
        }
        let hashed = hash(password, DEFAULT_COST)
            .map_err(|e| IoError::new(ErrorKind::Other, e))?;
        let user = User {
            username: username.to_string(),
            hashed_password: hashed,
            is_admin,
        };
        self.users.push(user);
        self.save_to_file("users.json")?;
        info!("Created new user: {}", username);
        Ok(())
    }

    pub fn log_users(&self) {
        info!("Registered users: {:?}", self.users);
    }
}

pub struct DfSessionService {
    pub session_context: Arc<SessionContext>,
    pub parser: Arc<PgQueryParser>,
    pub db: Arc<crate::database::Database>,
    pub user_db: Arc<Mutex<UserDB>>,
}

impl DfSessionService {
    pub fn new(session_context: SessionContext, db: Arc<crate::database::Database>) -> Self {
        let session_context = Arc::new(session_context);
        let parser = Arc::new(PgQueryParser {
            session_context: session_context.clone(),
        });
        let user_db = UserDB::load_from_file("users.json").unwrap_or_else(|e| {
            error!("Failed to load user DB: {}. Starting with empty DB.", e);
            UserDB { users: vec![] }
        });
        
        // Register table_events (example in-memory table)
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("event_name", DataType::Utf8, true),
        ]));
        let table = Arc::new(MemTable::try_new(schema, vec![]).unwrap());
        session_context.register_table("table_events", table).unwrap();

        Self {
            session_context,
            parser,
            db,
            user_db: Arc::new(Mutex::new(user_db)),
        }
    }

    pub async fn log_users_async(&self) {
        let user_db = self.user_db.lock().await;
        user_db.log_users();
    }
}

pub struct PgQueryParser {
    pub session_context: Arc<SessionContext>,
}

#[async_trait]
impl QueryParser for PgQueryParser {
    type Statement = LogicalPlan;

    async fn parse_sql(&self, sql: &str, _types: &[Type]) -> PgWireResult<Self::Statement> {
        let new_sql = prepare_sql(sql).map_err(|e| PgWireError::ApiError(e.into()))?;
        let state = self.session_context.state();
        let logical_plan = state.create_logical_plan(&new_sql)
            .await
            .map_err(|e| PgWireError::ApiError(e.into()))?;
        let optimised = state.optimize(&logical_plan)
            .map_err(|e| PgWireError::ApiError(e.into()))?;
        Ok(optimised)
    }
}

#[async_trait]
impl pgwire::api::query::SimpleQueryHandler for DfSessionService {
    async fn do_query<'a, C>(
        &self,
        _client: &mut C,
        query: &'a str,
    ) -> PgWireResult<Vec<Response<'a>>>
    where
        C: ClientInfo + SinkExt<PgWireBackendMessage> + Unpin + Send,
    {
        debug!("Received query: {}", query);
        let query_lower = query.trim_start().to_lowercase();
        debug!("Query lowercase: {}", query_lower);
        if query_lower.starts_with("insert") {
            debug!("Processing INSERT query");
            let msg = (&*self.db).insert_record(query)
                .await
                .map_err(|e| PgWireError::ApiError(e.into()))?;
            return Ok(vec![command_complete_response(&msg)]);
        } else if query_lower.starts_with("update") {
            debug!("Processing UPDATE query");
            let msg = (&*self.db).update_record(query)
                .await
                .map_err(|e| PgWireError::ApiError(e.into()))?;
            return Ok(vec![command_complete_response(&msg)]);
        } else if query_lower.starts_with("delete") {
            debug!("Processing DELETE query");
            let msg = (&*self.db).delete_record(query)
                .await
                .map_err(|e| PgWireError::ApiError(e.into()))?;
            return Ok(vec![command_complete_response(&msg)]);
        }

        debug!("Preparing SQL: {}", query);
        let new_sql = prepare_sql(query).map_err(|e| PgWireError::ApiError(e.into()))?;
        debug!("Executing SQL: {}", new_sql);
        let df = self.session_context.sql(&new_sql)
            .await
            .map_err(|e| PgWireError::ApiError(e.into()))?;
        debug!("Encoding DataFrame");
        let resp = encode_dataframe(df, &pgwire::api::portal::Format::UnifiedText)
            .await
            .map_err(|e| PgWireError::ApiError(e.into()))?;
        debug!("Query completed successfully");
        Ok(vec![Response::Query(resp)])
    }
}

#[async_trait]
impl pgwire::api::query::ExtendedQueryHandler for DfSessionService {
    type Statement = LogicalPlan;
    type QueryParser = PgQueryParser;

    fn query_parser(&self) -> Arc<Self::QueryParser> {
        self.parser.clone()
    }

    async fn do_describe_statement<C>(
        &self,
        _client: &mut C,
        target: &StoredStatement<Self::Statement>,
    ) -> PgWireResult<DescribeStatementResponse>
    where
        C: ClientInfo + SinkExt<PgWireBackendMessage> + Unpin + Send,
    {
        let plan = &target.statement;
        let schema = plan.schema();
        let fields = pgwire_schema_from_arrow(schema)?;
        let params = plan.get_parameter_types()
            .map_err(|e| PgWireError::ApiError(e.into()))?;
        let mut param_types = Vec::with_capacity(params.len());
        for param in ordered_param_types(&params).iter() {
            if let Some(dt) = param {
                let pgtype = into_pg_type(dt)
                    .map_err(|e| PgWireError::ApiError(e.into()))?;
                param_types.push(pgtype);
            } else {
                param_types.push(Type::UNKNOWN);
            }
        }
        Ok(DescribeStatementResponse::new(param_types, fields))
    }

    async fn do_describe_portal<C>(
        &self,
        _client: &mut C,
        target: &pgwire::api::portal::Portal<Self::Statement>,
    ) -> PgWireResult<DescribePortalResponse>
    where
        C: ClientInfo + SinkExt<PgWireBackendMessage> + Unpin + Send,
    {
        let plan = &target.statement.statement;
        let fields = pgwire_schema_from_arrow(plan.schema())?;
        Ok(DescribePortalResponse::new(fields))
    }

    async fn do_query<'a, C>(
        &self,
        _client: &mut C,
        portal: &'a pgwire::api::portal::Portal<Self::Statement>,
        _max_rows: usize,
    ) -> PgWireResult<Response<'a>>
    where
        C: ClientInfo + SinkExt<PgWireBackendMessage> + Unpin + Send,
    {
        let plan = &portal.statement.statement;
        let params = plan.get_parameter_types()
            .map_err(|e| PgWireError::ApiError(e.into()))?;
        let param_values = deserialize_parameters(portal, &ordered_param_types(&params))
            .map_err(|e| PgWireError::ApiError(e.into()))?;
        let plan_with_values = plan.clone().replace_params_with_values(&param_values)
            .map_err(|e| PgWireError::ApiError(e.into()))?;
        let df = self.session_context.execute_logical_plan(plan_with_values)
            .await
            .map_err(|e| PgWireError::ApiError(e.into()))?;
        let resp = encode_dataframe(df, &portal.result_column_format)
            .await
            .map_err(|e| PgWireError::ApiError(e.into()))?;
        Ok(Response::Query(resp))
    }
}

fn command_complete_response(msg: &str) -> Response<'static> {
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&(1_i16).to_be_bytes());
    let bytes = msg.as_bytes();
    buf.extend_from_slice(&(bytes.len() as i32).to_be_bytes());
    buf.extend_from_slice(bytes);
    let row_stream = futures::stream::iter(vec![Ok(DataRow::new(buf, 1))]);
    let fields = vec![FieldInfo::new(
        "CommandComplete".to_string(),
        None,
        None,
        Type::TEXT,
        pgwire::api::results::FieldFormat::Text,
    )];
    let qr = QueryResponse::new(fields.into(), row_stream);
    Response::Query(qr)
}

async fn encode_dataframe(
    df: DataFrame,
    _format: &pgwire::api::portal::Format,
) -> Result<QueryResponse<'static>, Box<dyn std::error::Error + Send + Sync>> {
    debug!("Starting encode_dataframe");
    let schema = (*df.schema()).clone();
    debug!("Collecting DataFrame");
    let batches = df.collect().await?;
    debug!("Converting schema to pgwire format");
    let fields = pgwire_schema_from_arrow(&schema)?;
    let mut all_rows = Vec::new();
    for batch in batches {
        debug!("Processing batch with {} rows", batch.num_rows());
        for row in 0..batch.num_rows() {
            let mut row_values = Vec::new();
            for col in 0..batch.num_columns() {
                let array = batch.column(col);
                let value = if array.is_null(row) {
                    None
                } else {
                    Some(value_to_string(array.as_ref(), row))
                };
                row_values.push(value);
            }
            all_rows.push(Ok(DataRow::new(serialize_row(row_values), batch.num_columns() as i16)));
        }
    }
    debug!("Creating QueryResponse with {} rows", all_rows.len());
    let row_stream = futures::stream::iter(all_rows);
    Ok(QueryResponse::new(fields.into(), row_stream))
}

fn serialize_row(row_values: Vec<Option<String>>) -> BytesMut {
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&(row_values.len() as i16).to_be_bytes());
    for value in row_values {
        match value {
            Some(v) => {
                let bytes = v.as_bytes();
                buf.extend_from_slice(&(bytes.len() as i32).to_be_bytes());
                buf.extend_from_slice(bytes);
            }
            None => {
                buf.extend_from_slice(&(-1i32).to_be_bytes());
            }
        }
    }
    buf
}

fn pgwire_schema_from_arrow(schema: &datafusion::common::DFSchema) -> Result<Vec<FieldInfo>, Box<dyn std::error::Error + Send + Sync>> {
    let mut fields = Vec::new();
    for field in schema.fields() {
        let pg_type = into_pg_type(field.data_type())?;
        fields.push(FieldInfo::new(field.name().to_string(), None, None, pg_type, pgwire::api::results::FieldFormat::Text));
    }
    Ok(fields)
}

fn into_pg_type(dt: &datafusion::arrow::datatypes::DataType) -> Result<Type, Box<dyn std::error::Error + Send + Sync>> {
    match dt {
        datafusion::arrow::datatypes::DataType::Utf8 => Ok(Type::TEXT),
        datafusion::arrow::datatypes::DataType::Timestamp(datafusion::arrow::datatypes::TimeUnit::Microsecond, _) => Ok(Type::TIMESTAMP),
        datafusion::arrow::datatypes::DataType::Int64 => Ok(Type::INT8),
        datafusion::arrow::datatypes::DataType::Int32 => Ok(Type::INT4),
        _ => Ok(Type::TEXT),
    }
}

fn deserialize_parameters<T>(
    _portal: &pgwire::api::portal::Portal<T>,
    _ordered: &Vec<Option<&datafusion::arrow::datatypes::DataType>>,
) -> Result<ParamValues, Box<dyn std::error::Error + Send + Sync>> {
    Ok(ParamValues::List(vec![]))
}

fn ordered_param_types(
    types: &HashMap<String, Option<datafusion::arrow::datatypes::DataType>>,
) -> Vec<Option<&datafusion::arrow::datatypes::DataType>> {
    types.values().map(|opt| opt.as_ref()).collect()
}

#[async_trait]
impl StartupHandler for DfSessionService {
    async fn on_startup<C>(
        &self,
        client: &mut C,
        msg: PgWireFrontendMessage,
    ) -> Result<(), PgWireError>
    where
        C: ClientInfo + SinkExt<PgWireBackendMessage> + Unpin + Send,
        C::Error: std::fmt::Debug,
    {
        debug!("Received Startup message: {:?}", msg);
        if let PgWireFrontendMessage::Startup(startup) = msg {
            let user = startup.parameters.get("user").map(|s| s.as_str()).unwrap_or("");
            let provided_password = startup.parameters.get("password").map(|s| s.as_str()).unwrap_or("");
            info!("Authenticating user '{}' (provided password length: {})", user, provided_password.len());
            if !provided_password.is_empty() {
                let user_db = self.user_db.lock().await;
                if user_db.verify_user(user, provided_password) {
                    client.send(PgWireBackendMessage::Authentication(Authentication::Ok))
                        .await
                        .map_err(|e| PgWireError::IoError(std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e))))?;
                    client.send(PgWireBackendMessage::ReadyForQuery(ReadyForQuery::new(TransactionStatus::Idle)))
                        .await
                        .map_err(|e| PgWireError::IoError(std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e))))?;
                    return Ok(());
                } else {
                    return Err(PgWireError::ApiError("Authentication failed".into()));
                }
            } else {
                if let Ok(fallback_password) = std::env::var("PGPASSWORD") {
                    let user_db = self.user_db.lock().await;
                    if user_db.verify_user(user, &fallback_password) {
                        info!("User '{}' authenticated using fallback password", user);
                        client.send(PgWireBackendMessage::Authentication(Authentication::Ok))
                            .await
                            .map_err(|e| PgWireError::IoError(std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e))))?;
                        client.send(PgWireBackendMessage::ReadyForQuery(ReadyForQuery::new(TransactionStatus::Idle)))
                            .await
                            .map_err(|e| PgWireError::IoError(std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e))))?;
                        return Ok(());
                    }
                }
                return Err(PgWireError::ApiError("No password provided".into()));
            }
        } else {
            return Err(PgWireError::ApiError("Expected Startup message".into()));
        }
    }
}

#[derive(Clone)]
pub struct HandlerFactory(pub Arc<DfSessionService>);

impl PgWireServerHandlers for HandlerFactory {
    type StartupHandler = DfSessionService;
    type SimpleQueryHandler = DfSessionService;
    type ExtendedQueryHandler = DfSessionService;
    type CopyHandler = NoopCopyHandler;
    type ErrorHandler = NoopErrorHandler;

    fn simple_query_handler(&self) -> Arc<Self::SimpleQueryHandler> {
        self.0.clone()
    }
    fn extended_query_handler(&self) -> Arc<Self::ExtendedQueryHandler> {
        self.0.clone()
    }
    fn startup_handler(&self) -> Arc<Self::StartupHandler> {
        self.0.clone()
    }
    fn copy_handler(&self) -> Arc<Self::CopyHandler> {
        Arc::new(NoopCopyHandler)
    }
    fn error_handler(&self) -> Arc<Self::ErrorHandler> {
        Arc::new(NoopErrorHandler)
    }
}

pub async fn run_pgwire_server<H>(
    handler: H,
    addr: &str,
    shutdown: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    H: PgWireServerHandlers + Clone + Send + Sync + 'static,
{
    use tokio::net::TcpListener;
    let listener = TcpListener::bind(addr).await?;
    info!("PGWire server listening on {}", addr);

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                info!("PGWire server shutting down");
                break;
            }
            result = listener.accept() => {
                match result {
                    Ok((socket, peer_addr)) => {
                        info!("Accepted connection from {:?}", peer_addr);
                        let handler_clone = handler.clone();
                        tokio::spawn(async move {
                            if let Err(e) = pgwire::tokio::process_socket(socket, None, handler_clone).await {
                                error!("PGWire connection error: {:?}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {:?}", e);
                    }
                }
            }
        }
    }
    Ok(())
}