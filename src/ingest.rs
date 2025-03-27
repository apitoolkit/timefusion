use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use actix_web::{get, post, web, HttpResponse, Responder};
use datafusion::arrow::record_batch::RecordBatch;
use futures::future::join_all;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{error, info};

use crate::{
    database::Database,
    persistent_queue::{IngestRecord, PersistentQueue},
};

#[derive(Clone)]
pub struct IngestStatusStore {
    pub inner: Arc<RwLock<HashMap<String, String>>>,
}

impl IngestStatusStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn set_status(&self, receipt: String, status: String) {
        let mut inner = self.inner.write().expect("RwLock poisoned");
        inner.insert(receipt, status);
    }

    pub fn get_status(&self, receipt: &str) -> Option<String> {
        let inner = self.inner.read().expect("RwLock poisoned");
        inner.get(receipt).cloned()
    }
}

#[derive(Deserialize)]
pub struct IngestData {
    pub trace_id: String,
    pub span_id: String,
    pub trace_state: Option<String>,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub kind: Option<String>,
    pub start_time_unix_nano: i64,
    pub end_time_unix_nano: Option<i64>,

    // Span attributes
    pub http_method: Option<String>,
    pub http_url: Option<String>,
    pub http_status_code: Option<i32>,
    pub http_request_content_length: Option<i64>,
    pub http_response_content_length: Option<i64>,
    pub http_route: Option<String>,
    pub http_scheme: Option<String>,
    pub http_client_ip: Option<String>,
    pub http_user_agent: Option<String>,
    pub http_flavor: Option<String>,
    pub http_target: Option<String>,
    pub http_host: Option<String>,
    pub rpc_system: Option<String>,
    pub rpc_service: Option<String>,
    pub rpc_method: Option<String>,
    pub rpc_grpc_status_code: Option<i32>,
    pub db_system: Option<String>,
    pub db_connection_string: Option<String>,
    pub db_user: Option<String>,
    pub db_name: Option<String>,
    pub db_statement: Option<String>,
    pub db_operation: Option<String>,
    pub db_sql_table: Option<String>,
    pub messaging_system: Option<String>,
    pub messaging_destination: Option<String>,
    pub messaging_destination_kind: Option<String>,
    pub messaging_message_id: Option<String>,
    pub messaging_operation: Option<String>,
    pub messaging_url: Option<String>,
    pub messaging_client_id: Option<String>,
    pub messaging_kafka_partition: Option<i32>,
    pub messaging_kafka_offset: Option<i64>,
    pub messaging_kafka_consumer_group: Option<String>,
    pub messaging_message_payload_size_bytes: Option<i64>,
    pub messaging_protocol: Option<String>,
    pub messaging_protocol_version: Option<String>,
    pub cache_system: Option<String>,
    pub cache_operation: Option<String>,
    pub cache_key: Option<String>,
    pub cache_hit: Option<bool>,
    pub net_peer_ip: Option<String>,
    pub net_peer_port: Option<i32>,
    pub net_host_ip: Option<String>,
    pub net_host_port: Option<i32>,
    pub net_transport: Option<String>,
    pub enduser_id: Option<String>,
    pub enduser_role: Option<String>,
    pub enduser_scope: Option<String>,
    pub exception_type: Option<String>,
    pub exception_message: Option<String>,
    pub exception_stacktrace: Option<String>,
    pub exception_escaped: Option<bool>,
    pub thread_id: Option<i64>,
    pub thread_name: Option<String>,
    pub code_function: Option<String>,
    pub code_filepath: Option<String>,
    pub code_namespace: Option<String>,
    pub code_lineno: Option<i32>,
    pub deployment_environment: Option<String>,
    pub deployment_version: Option<String>,
    pub service_name: Option<String>,
    pub service_version: Option<String>,
    pub service_instance_id: Option<String>,
    pub otel_library_name: Option<String>,
    pub otel_library_version: Option<String>,
    pub k8s_pod_name: Option<String>,
    pub k8s_namespace_name: Option<String>,
    pub k8s_deployment_name: Option<String>,
    pub container_id: Option<String>,
    pub host_name: Option<String>,
    pub os_type: Option<String>,
    pub os_version: Option<String>,
    pub process_pid: Option<i64>,
    pub process_command_line: Option<String>,
    pub process_runtime_name: Option<String>,
    pub process_runtime_version: Option<String>,
    pub aws_region: Option<String>,
    pub aws_account_id: Option<String>,
    pub aws_dynamodb_table_name: Option<String>,
    pub aws_dynamodb_operation: Option<String>,
    pub aws_dynamodb_consumed_capacity_total: Option<f64>,
    pub aws_sqs_queue_url: Option<String>,
    pub aws_sqs_message_id: Option<String>,
    pub azure_resource_id: Option<String>,
    pub azure_storage_container_name: Option<String>,
    pub azure_storage_blob_name: Option<String>,
    pub gcp_project_id: Option<String>,
    pub gcp_cloudsql_instance_id: Option<String>,
    pub gcp_pubsub_message_id: Option<String>,
    pub http_request_method: Option<String>,
    pub db_instance_identifier: Option<String>,
    pub db_rows_affected: Option<i64>,
    pub net_sock_peer_addr: Option<String>,
    pub net_sock_peer_port: Option<i32>,
    pub net_sock_host_addr: Option<String>,
    pub net_sock_host_port: Option<i32>,
    pub messaging_consumer_id: Option<String>,
    pub messaging_message_payload_compressed_size_bytes: Option<i64>,
    pub faas_invocation_id: Option<String>,
    pub faas_trigger: Option<String>,
    pub cloud_zone: Option<String>,

    // Resource attributes
    pub resource_attributes_service_name: Option<String>,
    pub resource_attributes_service_version: Option<String>,
    pub resource_attributes_service_instance_id: Option<String>,
    pub resource_attributes_service_namespace: Option<String>,
    pub resource_attributes_host_name: Option<String>,
    pub resource_attributes_host_id: Option<String>,
    pub resource_attributes_host_type: Option<String>,
    pub resource_attributes_host_arch: Option<String>,
    pub resource_attributes_os_type: Option<String>,
    pub resource_attributes_os_version: Option<String>,
    pub resource_attributes_process_pid: Option<i64>,
    pub resource_attributes_process_executable_name: Option<String>,
    pub resource_attributes_process_command_line: Option<String>,
    pub resource_attributes_process_runtime_name: Option<String>,
    pub resource_attributes_process_runtime_version: Option<String>,
    pub resource_attributes_process_runtime_description: Option<String>,
    pub resource_attributes_process_executable_path: Option<String>,
    pub resource_attributes_k8s_cluster_name: Option<String>,
    pub resource_attributes_k8s_namespace_name: Option<String>,
    pub resource_attributes_k8s_deployment_name: Option<String>,
    pub resource_attributes_k8s_pod_name: Option<String>,
    pub resource_attributes_k8s_pod_uid: Option<String>,
    pub resource_attributes_k8s_replicaset_name: Option<String>,
    pub resource_attributes_k8s_deployment_strategy: Option<String>,
    pub resource_attributes_k8s_container_name: Option<String>,
    pub resource_attributes_k8s_node_name: Option<String>,
    pub resource_attributes_container_id: Option<String>,
    pub resource_attributes_container_image_name: Option<String>,
    pub resource_attributes_container_image_tag: Option<String>,
    pub resource_attributes_deployment_environment: Option<String>,
    pub resource_attributes_deployment_version: Option<String>,
    pub resource_attributes_cloud_provider: Option<String>,
    pub resource_attributes_cloud_platform: Option<String>,
    pub resource_attributes_cloud_region: Option<String>,
    pub resource_attributes_cloud_availability_zone: Option<String>,
    pub resource_attributes_cloud_account_id: Option<String>,
    pub resource_attributes_cloud_resource_id: Option<String>,
    pub resource_attributes_cloud_instance_type: Option<String>,
    pub resource_attributes_telemetry_sdk_name: Option<String>,
    pub resource_attributes_telemetry_sdk_language: Option<String>,
    pub resource_attributes_telemetry_sdk_version: Option<String>,
    pub resource_attributes_application_name: Option<String>,
    pub resource_attributes_application_version: Option<String>,
    pub resource_attributes_application_tier: Option<String>,
    pub resource_attributes_application_owner: Option<String>,
    pub resource_attributes_customer_id: Option<String>,
    pub resource_attributes_tenant_id: Option<String>,
    pub resource_attributes_feature_flag_enabled: Option<bool>,
    pub resource_attributes_payment_gateway: Option<String>,
    pub resource_attributes_database_type: Option<String>,
    pub resource_attributes_database_instance: Option<String>,
    pub resource_attributes_cache_provider: Option<String>,
    pub resource_attributes_message_queue_type: Option<String>,
    pub resource_attributes_http_route: Option<String>,
    pub resource_attributes_aws_ecs_cluster_arn: Option<String>,
    pub resource_attributes_aws_ecs_container_arn: Option<String>,
    pub resource_attributes_aws_ecs_task_arn: Option<String>,
    pub resource_attributes_aws_ecs_task_family: Option<String>,
    pub resource_attributes_aws_ec2_instance_id: Option<String>,
    pub resource_attributes_gcp_project_id: Option<String>,
    pub resource_attributes_gcp_zone: Option<String>,
    pub resource_attributes_azure_resource_id: Option<String>,
    pub resource_attributes_dynatrace_entity_process_id: Option<String>,
    pub resource_attributes_elastic_node_name: Option<String>,
    pub resource_attributes_istio_mesh_id: Option<String>,
    pub resource_attributes_cloudfoundry_application_id: Option<String>,
    pub resource_attributes_cloudfoundry_space_id: Option<String>,
    pub resource_attributes_opentelemetry_collector_name: Option<String>,
    pub resource_attributes_instrumentation_name: Option<String>,
    pub resource_attributes_instrumentation_version: Option<String>,
    pub resource_attributes_log_source: Option<String>,

    // Nested structures
    pub events: Option<String>,
    pub links: Option<String>,
    pub status_code: Option<String>,
    pub status_message: Option<String>,
    pub instrumentation_library_name: Option<String>,
    pub instrumentation_library_version: Option<String>,
}

#[post("/ingest")]
pub async fn ingest(
    data: web::Json<IngestData>,
    _db: web::Data<Arc<Database>>,
    queue: web::Data<Arc<PersistentQueue>>,
    status_store: web::Data<Arc<IngestStatusStore>>,
) -> impl Responder {
    let record = IngestRecord {
        trace_id: data.trace_id.clone(),
        span_id: data.span_id.clone(),
        trace_state: data.trace_state.clone(),
        parent_span_id: data.parent_span_id.clone(),
        name: data.name.clone(),
        kind: data.kind.clone(),
        start_time_unix_nano: data.start_time_unix_nano,
        end_time_unix_nano: data.end_time_unix_nano,
        http_method: data.http_method.clone(),
        http_url: data.http_url.clone(),
        http_status_code: data.http_status_code,
        http_request_content_length: data.http_request_content_length,
        http_response_content_length: data.http_response_content_length,
        http_route: data.http_route.clone(),
        http_scheme: data.http_scheme.clone(),
        http_client_ip: data.http_client_ip.clone(),
        http_user_agent: data.http_user_agent.clone(),
        http_flavor: data.http_flavor.clone(),
        http_target: data.http_target.clone(),
        http_host: data.http_host.clone(),
        rpc_system: data.rpc_system.clone(),
        rpc_service: data.rpc_service.clone(),
        rpc_method: data.rpc_method.clone(),
        rpc_grpc_status_code: data.rpc_grpc_status_code,
        db_system: data.db_system.clone(),
        db_connection_string: data.db_connection_string.clone(),
        db_user: data.db_user.clone(),
        db_name: data.db_name.clone(),
        db_statement: data.db_statement.clone(),
        db_operation: data.db_operation.clone(),
        db_sql_table: data.db_sql_table.clone(),
        messaging_system: data.messaging_system.clone(),
        messaging_destination: data.messaging_destination.clone(),
        messaging_destination_kind: data.messaging_destination_kind.clone(),
        messaging_message_id: data.messaging_message_id.clone(),
        messaging_operation: data.messaging_operation.clone(),
        messaging_url: data.messaging_url.clone(),
        messaging_client_id: data.messaging_client_id.clone(),
        messaging_kafka_partition: data.messaging_kafka_partition,
        messaging_kafka_offset: data.messaging_kafka_offset,
        messaging_kafka_consumer_group: data.messaging_kafka_consumer_group.clone(),
        messaging_message_payload_size_bytes: data.messaging_message_payload_size_bytes,
        messaging_protocol: data.messaging_protocol.clone(),
        messaging_protocol_version: data.messaging_protocol_version.clone(),
        cache_system: data.cache_system.clone(),
        cache_operation: data.cache_operation.clone(),
        cache_key: data.cache_key.clone(),
        cache_hit: data.cache_hit,
        net_peer_ip: data.net_peer_ip.clone(),
        net_peer_port: data.net_peer_port,
        net_host_ip: data.net_host_ip.clone(),
        net_host_port: data.net_host_port,
        net_transport: data.net_transport.clone(),
        enduser_id: data.enduser_id.clone(),
        enduser_role: data.enduser_role.clone(),
        enduser_scope: data.enduser_scope.clone(),
        exception_type: data.exception_type.clone(),
        exception_message: data.exception_message.clone(),
        exception_stacktrace: data.exception_stacktrace.clone(),
        exception_escaped: data.exception_escaped,
        thread_id: data.thread_id,
        thread_name: data.thread_name.clone(),
        code_function: data.code_function.clone(),
        code_filepath: data.code_filepath.clone(),
        code_namespace: data.code_namespace.clone(),
        code_lineno: data.code_lineno,
        deployment_environment: data.deployment_environment.clone(),
        deployment_version: data.deployment_version.clone(),
        service_name: data.service_name.clone(),
        service_version: data.service_version.clone(),
        service_instance_id: data.service_instance_id.clone(),
        otel_library_name: data.otel_library_name.clone(),
        otel_library_version: data.otel_library_version.clone(),
        k8s_pod_name: data.k8s_pod_name.clone(),
        k8s_namespace_name: data.k8s_namespace_name.clone(),
        k8s_deployment_name: data.k8s_deployment_name.clone(),
        container_id: data.container_id.clone(),
        host_name: data.host_name.clone(),
        os_type: data.os_type.clone(),
        os_version: data.os_version.clone(),
        process_pid: data.process_pid,
        process_command_line: data.process_command_line.clone(),
        process_runtime_name: data.process_runtime_name.clone(),
        process_runtime_version: data.process_runtime_version.clone(),
        aws_region: data.aws_region.clone(),
        aws_account_id: data.aws_account_id.clone(),
        aws_dynamodb_table_name: data.aws_dynamodb_table_name.clone(),
        aws_dynamodb_operation: data.aws_dynamodb_operation.clone(),
        aws_dynamodb_consumed_capacity_total: data.aws_dynamodb_consumed_capacity_total,
        aws_sqs_queue_url: data.aws_sqs_queue_url.clone(),
        aws_sqs_message_id: data.aws_sqs_message_id.clone(),
        azure_resource_id: data.azure_resource_id.clone(),
        azure_storage_container_name: data.azure_storage_container_name.clone(),
        azure_storage_blob_name: data.azure_storage_blob_name.clone(),
        gcp_project_id: data.gcp_project_id.clone(),
        gcp_cloudsql_instance_id: data.gcp_cloudsql_instance_id.clone(),
        gcp_pubsub_message_id: data.gcp_pubsub_message_id.clone(),
        http_request_method: data.http_request_method.clone(),
        db_instance_identifier: data.db_instance_identifier.clone(),
        db_rows_affected: data.db_rows_affected,
        net_sock_peer_addr: data.net_sock_peer_addr.clone(),
        net_sock_peer_port: data.net_sock_peer_port,
        net_sock_host_addr: data.net_sock_host_addr.clone(),
        net_sock_host_port: data.net_sock_host_port,
        messaging_consumer_id: data.messaging_consumer_id.clone(),
        messaging_message_payload_compressed_size_bytes: data.messaging_message_payload_compressed_size_bytes,
        faas_invocation_id: data.faas_invocation_id.clone(),
        faas_trigger: data.faas_trigger.clone(),
        cloud_zone: data.cloud_zone.clone(),
        resource_attributes_service_name: data.resource_attributes_service_name.clone(),
        resource_attributes_service_version: data.resource_attributes_service_version.clone(),
        resource_attributes_service_instance_id: data.resource_attributes_service_instance_id.clone(),
        resource_attributes_service_namespace: data.resource_attributes_service_namespace.clone(),
        resource_attributes_host_name: data.resource_attributes_host_name.clone(),
        resource_attributes_host_id: data.resource_attributes_host_id.clone(),
        resource_attributes_host_type: data.resource_attributes_host_type.clone(),
        resource_attributes_host_arch: data.resource_attributes_host_arch.clone(),
        resource_attributes_os_type: data.resource_attributes_os_type.clone(),
        resource_attributes_os_version: data.resource_attributes_os_version.clone(),
        resource_attributes_process_pid: data.resource_attributes_process_pid,
        resource_attributes_process_executable_name: data.resource_attributes_process_executable_name.clone(),
        resource_attributes_process_command_line: data.resource_attributes_process_command_line.clone(),
        resource_attributes_process_runtime_name: data.resource_attributes_process_runtime_name.clone(),
        resource_attributes_process_runtime_version: data.resource_attributes_process_runtime_version.clone(),
        resource_attributes_process_runtime_description: data.resource_attributes_process_runtime_description.clone(),
        resource_attributes_process_executable_path: data.resource_attributes_process_executable_path.clone(),
        resource_attributes_k8s_cluster_name: data.resource_attributes_k8s_cluster_name.clone(),
        resource_attributes_k8s_namespace_name: data.resource_attributes_k8s_namespace_name.clone(),
        resource_attributes_k8s_deployment_name: data.resource_attributes_k8s_deployment_name.clone(),
        resource_attributes_k8s_pod_name: data.resource_attributes_k8s_pod_name.clone(),
        resource_attributes_k8s_pod_uid: data.resource_attributes_k8s_pod_uid.clone(),
        resource_attributes_k8s_replicaset_name: data.resource_attributes_k8s_replicaset_name.clone(),
        resource_attributes_k8s_deployment_strategy: data.resource_attributes_k8s_deployment_strategy.clone(),
        resource_attributes_k8s_container_name: data.resource_attributes_k8s_container_name.clone(),
        resource_attributes_k8s_node_name: data.resource_attributes_k8s_node_name.clone(),
        resource_attributes_container_id: data.resource_attributes_container_id.clone(),
        resource_attributes_container_image_name: data.resource_attributes_container_image_name.clone(),
        resource_attributes_container_image_tag: data.resource_attributes_container_image_tag.clone(),
        resource_attributes_deployment_environment: data.resource_attributes_deployment_environment.clone(),
        resource_attributes_deployment_version: data.resource_attributes_deployment_version.clone(),
        resource_attributes_cloud_provider: data.resource_attributes_cloud_provider.clone(),
        resource_attributes_cloud_platform: data.resource_attributes_cloud_platform.clone(),
        resource_attributes_cloud_region: data.resource_attributes_cloud_region.clone(),
        resource_attributes_cloud_availability_zone: data.resource_attributes_cloud_availability_zone.clone(),
        resource_attributes_cloud_account_id: data.resource_attributes_cloud_account_id.clone(),
        resource_attributes_cloud_resource_id: data.resource_attributes_cloud_resource_id.clone(),
        resource_attributes_cloud_instance_type: data.resource_attributes_cloud_instance_type.clone(),
        resource_attributes_telemetry_sdk_name: data.resource_attributes_telemetry_sdk_name.clone(),
        resource_attributes_telemetry_sdk_language: data.resource_attributes_telemetry_sdk_language.clone(),
        resource_attributes_telemetry_sdk_version: data.resource_attributes_telemetry_sdk_version.clone(),
        resource_attributes_application_name: data.resource_attributes_application_name.clone(),
        resource_attributes_application_version: data.resource_attributes_application_version.clone(),
        resource_attributes_application_tier: data.resource_attributes_application_tier.clone(),
        resource_attributes_application_owner: data.resource_attributes_application_owner.clone(),
        resource_attributes_customer_id: data.resource_attributes_customer_id.clone(),
        resource_attributes_tenant_id: data.resource_attributes_tenant_id.clone(),
        resource_attributes_feature_flag_enabled: data.resource_attributes_feature_flag_enabled,
        resource_attributes_payment_gateway: data.resource_attributes_payment_gateway.clone(),
        resource_attributes_database_type: data.resource_attributes_database_type.clone(),
        resource_attributes_database_instance: data.resource_attributes_database_instance.clone(),
        resource_attributes_cache_provider: data.resource_attributes_cache_provider.clone(),
        resource_attributes_message_queue_type: data.resource_attributes_message_queue_type.clone(),
        resource_attributes_http_route: data.resource_attributes_http_route.clone(),
        resource_attributes_aws_ecs_cluster_arn: data.resource_attributes_aws_ecs_cluster_arn.clone(),
        resource_attributes_aws_ecs_container_arn: data.resource_attributes_aws_ecs_container_arn.clone(),
        resource_attributes_aws_ecs_task_arn: data.resource_attributes_aws_ecs_task_arn.clone(),
        resource_attributes_aws_ecs_task_family: data.resource_attributes_aws_ecs_task_family.clone(),
        resource_attributes_aws_ec2_instance_id: data.resource_attributes_aws_ec2_instance_id.clone(),
        resource_attributes_gcp_project_id: data.resource_attributes_gcp_project_id.clone(),
        resource_attributes_gcp_zone: data.resource_attributes_gcp_zone.clone(),
        resource_attributes_azure_resource_id: data.resource_attributes_azure_resource_id.clone(),
        resource_attributes_dynatrace_entity_process_id: data.resource_attributes_dynatrace_entity_process_id.clone(),
        resource_attributes_elastic_node_name: data.resource_attributes_elastic_node_name.clone(),
        resource_attributes_istio_mesh_id: data.resource_attributes_istio_mesh_id.clone(),
        resource_attributes_cloudfoundry_application_id: data.resource_attributes_cloudfoundry_application_id.clone(),
        resource_attributes_cloudfoundry_space_id: data.resource_attributes_cloudfoundry_space_id.clone(),
        resource_attributes_opentelemetry_collector_name: data.resource_attributes_opentelemetry_collector_name.clone(),
        resource_attributes_instrumentation_name: data.resource_attributes_instrumentation_name.clone(),
        resource_attributes_instrumentation_version: data.resource_attributes_instrumentation_version.clone(),
        resource_attributes_log_source: data.resource_attributes_log_source.clone(),
        events: data.events.clone(),
        links: data.links.clone(),
        status_code: data.status_code.clone(),
        status_message: data.status_message.clone(),
        instrumentation_library_name: data.instrumentation_library_name.clone(),
        instrumentation_library_version: data.instrumentation_library_version.clone(),
    };

    match queue.enqueue(&record).await {
        Ok(receipt) => {
            status_store.set_status(receipt.clone(), "Enqueued".to_string());
            info!("Record enqueued with receipt: {}", receipt);
            HttpResponse::Ok().body(format!("Record enqueued. Receipt: {}", receipt))
        }
        Err(e) => {
            error!("Error enqueuing record: {:?}", e);
            HttpResponse::InternalServerError().body("Error enqueuing record")
        }
    }
}

#[post("/ingest_batch")]
pub async fn ingest_batch(
    data: web::Json<Vec<IngestData>>,
    _db: web::Data<Arc<Database>>,
    queue: web::Data<Arc<PersistentQueue>>,
    status_store: web::Data<Arc<IngestStatusStore>>,
) -> impl Responder {
    let records: Vec<IngestRecord> = data
        .iter()
        .map(|d| IngestRecord {
            trace_id: d.trace_id.clone(),
            span_id: d.span_id.clone(),
            trace_state: d.trace_state.clone(),
            parent_span_id: d.parent_span_id.clone(),
            name: d.name.clone(),
            kind: d.kind.clone(),
            start_time_unix_nano: d.start_time_unix_nano,
            end_time_unix_nano: d.end_time_unix_nano,
            http_method: d.http_method.clone(),
            http_url: d.http_url.clone(),
            http_status_code: d.http_status_code,
            http_request_content_length: d.http_request_content_length,
            http_response_content_length: d.http_response_content_length,
            http_route: d.http_route.clone(),
            http_scheme: d.http_scheme.clone(),
            http_client_ip: d.http_client_ip.clone(),
            http_user_agent: d.http_user_agent.clone(),
            http_flavor: d.http_flavor.clone(),
            http_target: d.http_target.clone(),
            http_host: d.http_host.clone(),
            rpc_system: d.rpc_system.clone(),
            rpc_service: d.rpc_service.clone(),
            rpc_method: d.rpc_method.clone(),
            rpc_grpc_status_code: d.rpc_grpc_status_code,
            db_system: d.db_system.clone(),
            db_connection_string: d.db_connection_string.clone(),
            db_user: d.db_user.clone(),
            db_name: d.db_name.clone(),
            db_statement: d.db_statement.clone(),
            db_operation: d.db_operation.clone(),
            db_sql_table: d.db_sql_table.clone(),
            messaging_system: d.messaging_system.clone(),
            messaging_destination: d.messaging_destination.clone(),
            messaging_destination_kind: d.messaging_destination_kind.clone(),
            messaging_message_id: d.messaging_message_id.clone(),
            messaging_operation: d.messaging_operation.clone(),
            messaging_url: d.messaging_url.clone(),
            messaging_client_id: d.messaging_client_id.clone(),
            messaging_kafka_partition: d.messaging_kafka_partition,
            messaging_kafka_offset: d.messaging_kafka_offset,
            messaging_kafka_consumer_group: d.messaging_kafka_consumer_group.clone(),
            messaging_message_payload_size_bytes: d.messaging_message_payload_size_bytes,
            messaging_protocol: d.messaging_protocol.clone(),
            messaging_protocol_version: d.messaging_protocol_version.clone(),
            cache_system: d.cache_system.clone(),
            cache_operation: d.cache_operation.clone(),
            cache_key: d.cache_key.clone(),
            cache_hit: d.cache_hit,
            net_peer_ip: d.net_peer_ip.clone(),
            net_peer_port: d.net_peer_port,
            net_host_ip: d.net_host_ip.clone(),
            net_host_port: d.net_host_port,
            net_transport: d.net_transport.clone(),
            enduser_id: d.enduser_id.clone(),
            enduser_role: d.enduser_role.clone(),
            enduser_scope: d.enduser_scope.clone(),
            exception_type: d.exception_type.clone(),
            exception_message: d.exception_message.clone(),
            exception_stacktrace: d.exception_stacktrace.clone(),
            exception_escaped: d.exception_escaped,
            thread_id: d.thread_id,
            thread_name: d.thread_name.clone(),
            code_function: d.code_function.clone(),
            code_filepath: d.code_filepath.clone(),
            code_namespace: d.code_namespace.clone(),
            code_lineno: d.code_lineno,
            deployment_environment: d.deployment_environment.clone(),
            deployment_version: d.deployment_version.clone(),
            service_name: d.service_name.clone(),
            service_version: d.service_version.clone(),
            service_instance_id: d.service_instance_id.clone(),
            otel_library_name: d.otel_library_name.clone(),
            otel_library_version: d.otel_library_version.clone(),
            k8s_pod_name: d.k8s_pod_name.clone(),
            k8s_namespace_name: d.k8s_namespace_name.clone(),
            k8s_deployment_name: d.k8s_deployment_name.clone(),
            container_id: d.container_id.clone(),
            host_name: d.host_name.clone(),
            os_type: d.os_type.clone(),
            os_version: d.os_version.clone(),
            process_pid: d.process_pid,
            process_command_line: d.process_command_line.clone(),
            process_runtime_name: d.process_runtime_name.clone(),
            process_runtime_version: d.process_runtime_version.clone(),
            aws_region: d.aws_region.clone(),
            aws_account_id: d.aws_account_id.clone(),
            aws_dynamodb_table_name: d.aws_dynamodb_table_name.clone(),
            aws_dynamodb_operation: d.aws_dynamodb_operation.clone(),
            aws_dynamodb_consumed_capacity_total: d.aws_dynamodb_consumed_capacity_total,
            aws_sqs_queue_url: d.aws_sqs_queue_url.clone(),
            aws_sqs_message_id: d.aws_sqs_message_id.clone(),
            azure_resource_id: d.azure_resource_id.clone(),
            azure_storage_container_name: d.azure_storage_container_name.clone(),
            azure_storage_blob_name: d.azure_storage_blob_name.clone(),
            gcp_project_id: d.gcp_project_id.clone(),
            gcp_cloudsql_instance_id: d.gcp_cloudsql_instance_id.clone(),
            gcp_pubsub_message_id: d.gcp_pubsub_message_id.clone(),
            http_request_method: d.http_request_method.clone(),
            db_instance_identifier: d.db_instance_identifier.clone(),
            db_rows_affected: d.db_rows_affected,
            net_sock_peer_addr: d.net_sock_peer_addr.clone(),
            net_sock_peer_port: d.net_sock_peer_port,
            net_sock_host_addr: d.net_sock_host_addr.clone(),
            net_sock_host_port: d.net_sock_host_port,
            messaging_consumer_id: d.messaging_consumer_id.clone(),
            messaging_message_payload_compressed_size_bytes: d.messaging_message_payload_compressed_size_bytes,
            faas_invocation_id: d.faas_invocation_id.clone(),
            faas_trigger: d.faas_trigger.clone(),
            cloud_zone: d.cloud_zone.clone(),
            resource_attributes_service_name: d.resource_attributes_service_name.clone(),
            resource_attributes_service_version: d.resource_attributes_service_version.clone(),
            resource_attributes_service_instance_id: d.resource_attributes_service_instance_id.clone(),
            resource_attributes_service_namespace: d.resource_attributes_service_namespace.clone(),
            resource_attributes_host_name: d.resource_attributes_host_name.clone(),
            resource_attributes_host_id: d.resource_attributes_host_id.clone(),
            resource_attributes_host_type: d.resource_attributes_host_type.clone(),
            resource_attributes_host_arch: d.resource_attributes_host_arch.clone(),
            resource_attributes_os_type: d.resource_attributes_os_type.clone(),
            resource_attributes_os_version: d.resource_attributes_os_version.clone(),
            resource_attributes_process_pid: d.resource_attributes_process_pid,
            resource_attributes_process_executable_name: d.resource_attributes_process_executable_name.clone(),
            resource_attributes_process_command_line: d.resource_attributes_process_command_line.clone(),
            resource_attributes_process_runtime_name: d.resource_attributes_process_runtime_name.clone(),
            resource_attributes_process_runtime_version: d.resource_attributes_process_runtime_version.clone(),
            resource_attributes_process_runtime_description: d.resource_attributes_process_runtime_description.clone(),
            resource_attributes_process_executable_path: d.resource_attributes_process_executable_path.clone(),
            resource_attributes_k8s_cluster_name: d.resource_attributes_k8s_cluster_name.clone(),
            resource_attributes_k8s_namespace_name: d.resource_attributes_k8s_namespace_name.clone(),
            resource_attributes_k8s_deployment_name: d.resource_attributes_k8s_deployment_name.clone(),
            resource_attributes_k8s_pod_name: d.resource_attributes_k8s_pod_name.clone(),
            resource_attributes_k8s_pod_uid: d.resource_attributes_k8s_pod_uid.clone(),
            resource_attributes_k8s_replicaset_name: d.resource_attributes_k8s_replicaset_name.clone(),
            resource_attributes_k8s_deployment_strategy: d.resource_attributes_k8s_deployment_strategy.clone(),
            resource_attributes_k8s_container_name: d.resource_attributes_k8s_container_name.clone(),
            resource_attributes_k8s_node_name: d.resource_attributes_k8s_node_name.clone(),
            resource_attributes_container_id: d.resource_attributes_container_id.clone(),
            resource_attributes_container_image_name: d.resource_attributes_container_image_name.clone(),
            resource_attributes_container_image_tag: d.resource_attributes_container_image_tag.clone(),
            resource_attributes_deployment_environment: d.resource_attributes_deployment_environment.clone(),
            resource_attributes_deployment_version: d.resource_attributes_deployment_version.clone(),
            resource_attributes_cloud_provider: d.resource_attributes_cloud_provider.clone(),
            resource_attributes_cloud_platform: d.resource_attributes_cloud_platform.clone(),
            resource_attributes_cloud_region: d.resource_attributes_cloud_region.clone(),
            resource_attributes_cloud_availability_zone: d.resource_attributes_cloud_availability_zone.clone(),
            resource_attributes_cloud_account_id: d.resource_attributes_cloud_account_id.clone(),
            resource_attributes_cloud_resource_id: d.resource_attributes_cloud_resource_id.clone(),
            resource_attributes_cloud_instance_type: d.resource_attributes_cloud_instance_type.clone(),
            resource_attributes_telemetry_sdk_name: d.resource_attributes_telemetry_sdk_name.clone(),
            resource_attributes_telemetry_sdk_language: d.resource_attributes_telemetry_sdk_language.clone(),
            resource_attributes_telemetry_sdk_version: d.resource_attributes_telemetry_sdk_version.clone(),
            resource_attributes_application_name: d.resource_attributes_application_name.clone(),
            resource_attributes_application_version: d.resource_attributes_application_version.clone(),
            resource_attributes_application_tier: d.resource_attributes_application_tier.clone(),
            resource_attributes_application_owner: d.resource_attributes_application_owner.clone(),
            resource_attributes_customer_id: d.resource_attributes_customer_id.clone(),
            resource_attributes_tenant_id: d.resource_attributes_tenant_id.clone(),
            resource_attributes_feature_flag_enabled: d.resource_attributes_feature_flag_enabled,
            resource_attributes_payment_gateway: d.resource_attributes_payment_gateway.clone(),
            resource_attributes_database_type: d.resource_attributes_database_type.clone(),
            resource_attributes_database_instance: d.resource_attributes_database_instance.clone(),
            resource_attributes_cache_provider: d.resource_attributes_cache_provider.clone(),
            resource_attributes_message_queue_type: d.resource_attributes_message_queue_type.clone(),
            resource_attributes_http_route: d.resource_attributes_http_route.clone(),
            resource_attributes_aws_ecs_cluster_arn: d.resource_attributes_aws_ecs_cluster_arn.clone(),
            resource_attributes_aws_ecs_container_arn: d.resource_attributes_aws_ecs_container_arn.clone(),
            resource_attributes_aws_ecs_task_arn: d.resource_attributes_aws_ecs_task_arn.clone(),
            resource_attributes_aws_ecs_task_family: d.resource_attributes_aws_ecs_task_family.clone(),
            resource_attributes_aws_ec2_instance_id: d.resource_attributes_aws_ec2_instance_id.clone(),
            resource_attributes_gcp_project_id: d.resource_attributes_gcp_project_id.clone(),
            resource_attributes_gcp_zone: d.resource_attributes_gcp_zone.clone(),
            resource_attributes_azure_resource_id: d.resource_attributes_azure_resource_id.clone(),
            resource_attributes_dynatrace_entity_process_id: d.resource_attributes_dynatrace_entity_process_id.clone(),
            resource_attributes_elastic_node_name: d.resource_attributes_elastic_node_name.clone(),
            resource_attributes_istio_mesh_id: d.resource_attributes_istio_mesh_id.clone(),
            resource_attributes_cloudfoundry_application_id: d.resource_attributes_cloudfoundry_application_id.clone(),
            resource_attributes_cloudfoundry_space_id: d.resource_attributes_cloudfoundry_space_id.clone(),
            resource_attributes_opentelemetry_collector_name: d.resource_attributes_opentelemetry_collector_name.clone(),
            resource_attributes_instrumentation_name: d.resource_attributes_instrumentation_name.clone(),
            resource_attributes_instrumentation_version: d.resource_attributes_instrumentation_version.clone(),
            resource_attributes_log_source: d.resource_attributes_log_source.clone(),
            events: d.events.clone(),
            links: d.links.clone(),
            status_code: d.status_code.clone(),
            status_message: d.status_message.clone(),
            instrumentation_library_name: d.instrumentation_library_name.clone(),
            instrumentation_library_version: d.instrumentation_library_version.clone(),
        })
        .collect();

    let futures: Vec<_> = records
        .iter()
        .map(|record| queue.enqueue(record))
        .collect();

    let results = join_all(futures).await;
    let mut receipts = Vec::new();
    let mut errors = Vec::new();

    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok(receipt) => {
                status_store.set_status(receipt.clone(), "Enqueued".to_string());
                receipts.push(receipt);
                info!("Record {} enqueued with receipt: {}", i, receipts.last().unwrap());
            }
            Err(e) => {
                errors.push(format!("Record {} failed: {:?}", i, e));
                error!("Error enqueuing record {}: {:?}", i, e);
            }
        }
    }

    if errors.is_empty() {
        HttpResponse::Ok().body(format!("Batch enqueued. Receipts: {:?}", receipts))
    } else {
        HttpResponse::InternalServerError().body(format!(
            "Errors occurred during batch ingestion: {:?}\nSuccessful receipts: {:?}",
            errors, receipts
        ))
    }
}

#[get("/status/{receipt}")]
pub async fn get_status(
    path: web::Path<String>,
    status_store: web::Data<Arc<IngestStatusStore>>,
) -> impl Responder {
    let receipt = path.into_inner();
    match status_store.get_status(&receipt) {
        Some(status) => HttpResponse::Ok().json(json!({ "receipt": receipt, "status": status })),
        None => HttpResponse::NotFound().body(format!("No status found for receipt: {}", receipt)),
    }
}

#[get("/queue_length")]
pub async fn queue_length(queue: web::Data<Arc<PersistentQueue>>) -> impl Responder {
    match queue.len() {
        Ok(length) => HttpResponse::Ok().json(json!({ "queue_length": length })),
        Err(e) => {
            error!("Error getting queue length: {:?}", e);
            HttpResponse::InternalServerError().body("Error getting queue length")
        }
    }
}

#[get("/data")]
pub async fn get_all_data(db: web::Data<Arc<Database>>) -> impl Responder {
    let query = "SELECT projectId, id, timestamp, traceId, spanId, eventType, durationNs FROM telemetry_events";
    match db.query(query).await {
        Ok(df) => {
            match df.collect().await {
                Ok(batches) => {
                    let json_rows = record_batches_to_json_rows(&batches).unwrap_or_default();
                    HttpResponse::Ok().json(json_rows)
                }
                Err(e) => {
                    error!("Error collecting data: {:?}", e);
                    HttpResponse::InternalServerError().body("Error collecting data")
                }
            }
        }
        Err(e) => {
            error!("Error querying data: {:?}", e);
            HttpResponse::InternalServerError().body("Error querying data")
        }
    }
}

#[get("/data/{id}")]
pub async fn get_data_by_id(
    path: web::Path<String>,
    db: web::Data<Arc<Database>>,
) -> impl Responder {
    let id = path.into_inner();
    let query = format!(
        "SELECT projectId, id, timestamp, traceId, spanId, eventType, durationNs FROM telemetry_events WHERE id = '{}'",
        id
    );
    match db.query(&query).await {
        Ok(df) => {
            match df.collect().await {
                Ok(batches) => {
                    let json_rows = record_batches_to_json_rows(&batches).unwrap_or_default();
                    if json_rows.is_empty() {
                        HttpResponse::NotFound().body(format!("No data found for id: {}", id))
                    } else {
                        HttpResponse::Ok().json(json_rows)
                    }
                }
                Err(e) => {
                    error!("Error collecting data for id {}: {:?}", id, e);
                    HttpResponse::InternalServerError().body("Error collecting data")
                }
            }
        }
        Err(e) => {
            error!("Error querying data for id {}: {:?}", id, e);
            HttpResponse::InternalServerError().body("Error querying data")
        }
    }
}

pub fn record_batches_to_json_rows(batches: &[RecordBatch]) -> Result<Vec<Value>, anyhow::Error> {
    let mut rows = Vec::new();
    for batch in batches {
        let schema = batch.schema();
        let num_rows = batch.num_rows();
        for row_idx in 0..num_rows {
            let mut row = json!({});
            for (col_idx, field) in schema.fields().iter().enumerate() {
                let column = batch.column(col_idx);
                let value = if column.is_null(row_idx) {
                    Value::Null
                } else {
                    match column.data_type() {
                        datafusion::arrow::datatypes::DataType::Int32 => {
                            column
                                .as_any()
                                .downcast_ref::<datafusion::arrow::array::Int32Array>()
                                .map_or(Value::Null, |arr| Value::Number(arr.value(row_idx).into()))
                        }
                        datafusion::arrow::datatypes::DataType::Int64 => {
                            column
                                .as_any()
                                .downcast_ref::<datafusion::arrow::array::Int64Array>()
                                .map_or(Value::Null, |arr| Value::Number(arr.value(row_idx).into()))
                        }
                        datafusion::arrow::datatypes::DataType::Float64 => {
                            column
                                .as_any()
                                .downcast_ref::<datafusion::arrow::array::Float64Array>()
                                .map_or(Value::Null, |arr| {
                                    Value::Number(
                                        serde_json::Number::from_f64(arr.value(row_idx))
                                            .unwrap_or_else(|| serde_json::Number::from(0)),
                                    )
                                })
                        }
                        datafusion::arrow::datatypes::DataType::Utf8 => {
                            column
                                .as_any()
                                .downcast_ref::<datafusion::arrow::array::StringArray>()
                                .map_or(Value::Null, |arr| Value::String(arr.value(row_idx).to_string()))
                        }
                        datafusion::arrow::datatypes::DataType::Timestamp(_, _) => {
                            column
                                .as_any()
                                .downcast_ref::<datafusion::arrow::array::TimestampNanosecondArray>()
                                .map_or(Value::Null, |arr| Value::String(arr.value(row_idx).to_string()))
                        }
                        _ => {
                            // Fallback for unsupported types
                            Value::Null
                        }
                    }
                };
                row[field.name()] = value;
            }
            rows.push(row);
        }
    }
    Ok(rows)
}