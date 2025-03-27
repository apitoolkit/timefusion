use std::sync::Arc as StdArc;
use datafusion::arrow::datatypes::{DataType, TimeUnit};

macro_rules! define_telemetry_fields {
    ($field_macro:ident) => {
        $field_macro!(trace_id, Utf8, false, String);
        $field_macro!(span_id, Utf8, false, String);
        $field_macro!(trace_state, Utf8, true, Option<String>);
        $field_macro!(parent_span_id, Utf8, true, Option<String>);
        $field_macro!(name, Utf8, false, String);
        $field_macro!(kind, Utf8, true, Option<String>);
        $field_macro!(start_time_unix_nano, Timestamp(TimeUnit::Nanosecond, Some(StdArc::from("UTC"))), false, i64);
        $field_macro!(end_time_unix_nano, Timestamp(TimeUnit::Nanosecond, Some(StdArc::from("UTC"))), true, Option<i64>);
        $field_macro!(span___http_method, Utf8, true, Option<String>);
        $field_macro!(span___http_url, Utf8, true, Option<String>);
        $field_macro!(span___http_status_code, Int32, true, Option<i32>);
        $field_macro!(span___http_request_content_length, Int64, true, Option<i64>);
        $field_macro!(span___http_response_content_length, Int64, true, Option<i64>);
        $field_macro!(span___http_route, Utf8, true, Option<String>);
        $field_macro!(span___http_scheme, Utf8, true, Option<String>);
        $field_macro!(span___http_client_ip, Utf8, true, Option<String>);
        $field_macro!(span___http_user_agent, Utf8, true, Option<String>);
        $field_macro!(span___http_flavor, Utf8, true, Option<String>);
        $field_macro!(span___http_target, Utf8, true, Option<String>);
        $field_macro!(span___http_host, Utf8, true, Option<String>);
        $field_macro!(span___rpc_system, Utf8, true, Option<String>);
        $field_macro!(span___rpc_service, Utf8, true, Option<String>);
        $field_macro!(span___rpc_method, Utf8, true, Option<String>);
        $field_macro!(span___rpc_grpc_status_code, Int32, true, Option<i32>);
        $field_macro!(span___db_system, Utf8, true, Option<String>);
        $field_macro!(span___db_connection_string, Utf8, true, Option<String>);
        $field_macro!(span___db_user, Utf8, true, Option<String>);
        $field_macro!(span___db_name, Utf8, true, Option<String>);
        $field_macro!(span___db_statement, Utf8, true, Option<String>);
        $field_macro!(span___db_operation, Utf8, true, Option<String>);
        $field_macro!(span___db_sql_table, Utf8, true, Option<String>);
        $field_macro!(span___messaging_system, Utf8, true, Option<String>);
        $field_macro!(span___messaging_destination, Utf8, true, Option<String>);
        $field_macro!(span___messaging_destination_kind, Utf8, true, Option<String>);
        $field_macro!(span___messaging_message_id, Utf8, true, Option<String>);
        $field_macro!(span___messaging_operation, Utf8, true, Option<String>);
        $field_macro!(span___messaging_url, Utf8, true, Option<String>);
        $field_macro!(span___messaging_client_id, Utf8, true, Option<String>);
        $field_macro!(span___messaging_kafka_partition, Int32, true, Option<i32>);
        $field_macro!(span___messaging_kafka_offset, Int64, true, Option<i64>);
        $field_macro!(span___messaging_kafka_consumer_group, Utf8, true, Option<String>);
        $field_macro!(span___messaging_message_payload_size_bytes, Int64, true, Option<i64>);
        $field_macro!(span___messaging_protocol, Utf8, true, Option<String>);
        $field_macro!(span___messaging_protocol_version, Utf8, true, Option<String>);
        $field_macro!(span___cache_system, Utf8, true, Option<String>);
        $field_macro!(span___cache_operation, Utf8, true, Option<String>);
        $field_macro!(span___cache_key, Utf8, true, Option<String>);
        $field_macro!(span___cache_hit, Boolean, true, Option<bool>);
        $field_macro!(span___net_peer_ip, Utf8, true, Option<String>);
        $field_macro!(span___net_peer_port, Int32, true, Option<i32>);
        $field_macro!(span___net_host_ip, Utf8, true, Option<String>);
        $field_macro!(span___net_host_port, Int32, true, Option<i32>);
        $field_macro!(span___net_transport, Utf8, true, Option<String>);
        $field_macro!(span___enduser_id, Utf8, true, Option<String>);
        $field_macro!(span___enduser_role, Utf8, true, Option<String>);
        $field_macro!(span___enduser_scope, Utf8, true, Option<String>);
        $field_macro!(span___exception_type, Utf8, true, Option<String>);
        $field_macro!(span___exception_message, Utf8, true, Option<String>);
        $field_macro!(span___exception_stacktrace, Utf8, true, Option<String>);
        $field_macro!(span___exception_escaped, Boolean, true, Option<bool>);
        $field_macro!(span___thread_id, Int64, true, Option<i64>);
        $field_macro!(span___thread_name, Utf8, true, Option<String>);
        $field_macro!(span___code_function, Utf8, true, Option<String>);
        $field_macro!(span___code_filepath, Utf8, true, Option<String>);
        $field_macro!(span___code_namespace, Utf8, true, Option<String>);
        $field_macro!(span___code_lineno, Int32, true, Option<i32>);
        $field_macro!(span___deployment_environment, Utf8, true, Option<String>);
        $field_macro!(span___deployment_version, Utf8, true, Option<String>);
        $field_macro!(span___service_name, Utf8, true, Option<String>);
        $field_macro!(span___service_version, Utf8, true, Option<String>);
        $field_macro!(span___service_instance_id, Utf8, true, Option<String>);
        $field_macro!(span___otel_library_name, Utf8, true, Option<String>);
        $field_macro!(span___otel_library_version, Utf8, true, Option<String>);
        $field_macro!(span___k8s_pod_name, Utf8, true, Option<String>);
        $field_macro!(span___k8s_namespace_name, Utf8, true, Option<String>);
        $field_macro!(span___k8s_deployment_name, Utf8, true, Option<String>);
        $field_macro!(span___container_id, Utf8, true, Option<String>);
        $field_macro!(span___host_name, Utf8, true, Option<String>);
        $field_macro!(span___os_type, Utf8, true, Option<String>);
        $field_macro!(span___os_version, Utf8, true, Option<String>);
        $field_macro!(span___process_pid, Int64, true, Option<i64>);
        $field_macro!(span___process_command_line, Utf8, true, Option<String>);
        $field_macro!(span___process_runtime_name, Utf8, true, Option<String>);
        $field_macro!(span___process_runtime_version, Utf8, true, Option<String>);
        $field_macro!(span___aws_region, Utf8, true, Option<String>);
        $field_macro!(span___aws_account_id, Utf8, true, Option<String>);
        $field_macro!(span___aws_dynamodb_table_name, Utf8, true, Option<String>);
        $field_macro!(span___aws_dynamodb_operation, Utf8, true, Option<String>);
        $field_macro!(span___aws_dynamodb_consumed_capacity_total, Float64, true, Option<f64>);
        $field_macro!(span___aws_sqs_queue_url, Utf8, true, Option<String>);
        $field_macro!(span___aws_sqs_message_id, Utf8, true, Option<String>);
        $field_macro!(span___azure_resource_id, Utf8, true, Option<String>);
        $field_macro!(span___azure_storage_container_name, Utf8, true, Option<String>);
        $field_macro!(span___azure_storage_blob_name, Utf8, true, Option<String>);
        $field_macro!(span___gcp_project_id, Utf8, true, Option<String>);
        $field_macro!(span___gcp_cloudsql_instance_id, Utf8, true, Option<String>);
        $field_macro!(span___gcp_pubsub_message_id, Utf8, true, Option<String>);
        $field_macro!(span___http_request_method, Utf8, true, Option<String>);
        $field_macro!(span___db_instance_identifier, Utf8, true, Option<String>);
        $field_macro!(span___db_rows_affected, Int64, true, Option<i64>);
        $field_macro!(span___net_sock_peer_addr, Utf8, true, Option<String>);
        $field_macro!(span___net_sock_peer_port, Int32, true, Option<i32>);
        $field_macro!(span___net_sock_host_addr, Utf8, true, Option<String>);
        $field_macro!(span___net_sock_host_port, Int32, true, Option<i32>);
        $field_macro!(span___messaging_consumer_id, Utf8, true, Option<String>);
        $field_macro!(span___messaging_message_payload_compressed_size_bytes, Int64, true, Option<i64>);
        $field_macro!(span___faas_invocation_id, Utf8, true, Option<String>);
        $field_macro!(span___faas_trigger, Utf8, true, Option<String>);
        $field_macro!(span___cloud_zone, Utf8, true, Option<String>);
        $field_macro!(attributes____service_name, Utf8, true, Option<String>);
        $field_macro!(attributes____service_version, Utf8, true, Option<String>);
        $field_macro!(attributes____service_instance_id, Utf8, true, Option<String>);
        $field_macro!(attributes____service_namespace, Utf8, true, Option<String>);
        $field_macro!(attributes____host_name, Utf8, true, Option<String>);
        $field_macro!(attributes____host_id, Utf8, true, Option<String>);
        $field_macro!(attributes____host_type, Utf8, true, Option<String>);
        $field_macro!(attributes____host_arch, Utf8, true, Option<String>);
        $field_macro!(attributes____os_type, Utf8, true, Option<String>);
        $field_macro!(attributes____os_version, Utf8, true, Option<String>);
        $field_macro!(attributes____process_pid, Int64, true, Option<i64>);
        $field_macro!(attributes____process_executable_name, Utf8, true, Option<String>);
        $field_macro!(attributes____process_command_line, Utf8, true, Option<String>);
        $field_macro!(attributes____process_runtime_name, Utf8, true, Option<String>);
        $field_macro!(attributes____process_runtime_version, Utf8, true, Option<String>);
        $field_macro!(attributes____process_runtime_description, Utf8, true, Option<String>);
        $field_macro!(attributes____process_executable_path, Utf8, true, Option<String>);
        $field_macro!(attributes____k8s_cluster_name, Utf8, true, Option<String>);
        $field_macro!(attributes____k8s_namespace_name, Utf8, true, Option<String>);
        $field_macro!(attributes____k8s_deployment_name, Utf8, true, Option<String>);
        $field_macro!(attributes____k8s_pod_name, Utf8, true, Option<String>);
        $field_macro!(attributes____k8s_pod_uid, Utf8, true, Option<String>);
        $field_macro!(attributes____k8s_replicaset_name, Utf8, true, Option<String>);
        $field_macro!(attributes____k8s_deployment_strategy, Utf8, true, Option<String>);
        $field_macro!(attributes____k8s_container_name, Utf8, true, Option<String>);
        $field_macro!(attributes____k8s_node_name, Utf8, true, Option<String>);
        $field_macro!(attributes____container_id, Utf8, true, Option<String>);
        $field_macro!(attributes____container_image_name, Utf8, true, Option<String>);
        $field_macro!(attributes____container_image_tag, Utf8, true, Option<String>);
        $field_macro!(attributes____deployment_environment, Utf8, true, Option<String>);
        $field_macro!(attributes____deployment_version, Utf8, true, Option<String>);
        $field_macro!(attributes____cloud_provider, Utf8, true, Option<String>);
        $field_macro!(attributes____cloud_platform, Utf8, true, Option<String>);
        $field_macro!(attributes____cloud_region, Utf8, true, Option<String>);
        $field_macro!(attributes____cloud_availability_zone, Utf8, true, Option<String>);
        $field_macro!(attributes____cloud_account_id, Utf8, true, Option<String>);
        $field_macro!(attributes____cloud_resource_id, Utf8, true, Option<String>);
        $field_macro!(attributes____cloud_instance_type, Utf8, true, Option<String>);
        $field_macro!(attributes____telemetry_sdk_name, Utf8, true, Option<String>);
        $field_macro!(attributes____telemetry_sdk_language, Utf8, true, Option<String>);
        $field_macro!(attributes____telemetry_sdk_version, Utf8, true, Option<String>);
        $field_macro!(attributes____application_name, Utf8, true, Option<String>);
        $field_macro!(attributes____application_version, Utf8, true, Option<String>);
        $field_macro!(attributes____application_tier, Utf8, true, Option<String>);
        $field_macro!(attributes____application_owner, Utf8, true, Option<String>);
        $field_macro!(attributes____customer_id, Utf8, true, Option<String>);
        $field_macro!(attributes____tenant_id, Utf8, true, Option<String>);
        $field_macro!(attributes____feature_flag_enabled, Boolean, true, Option<bool>);
        $field_macro!(attributes____payment_gateway, Utf8, true, Option<String>);
        $field_macro!(attributes____database_type, Utf8, true, Option<String>);
        $field_macro!(attributes____database_instance, Utf8, true, Option<String>);
        $field_macro!(attributes____cache_provider, Utf8, true, Option<String>);
        $field_macro!(attributes____message_queue_type, Utf8, true, Option<String>);
        $field_macro!(attributes____http_route, Utf8, true, Option<String>);
        $field_macro!(attributes____aws_ecs_cluster_arn, Utf8, true, Option<String>);
        $field_macro!(attributes____aws_ecs_container_arn, Utf8, true, Option<String>);
        $field_macro!(attributes____aws_ecs_task_arn, Utf8, true, Option<String>);
        $field_macro!(attributes____aws_ecs_task_family, Utf8, true, Option<String>);
        $field_macro!(attributes____aws_ec2_instance_id, Utf8, true, Option<String>);
        $field_macro!(attributes____gcp_project_id, Utf8, true, Option<String>);
        $field_macro!(attributes____gcp_zone, Utf8, true, Option<String>);
        $field_macro!(attributes____azure_resource_id, Utf8, true, Option<String>);
        $field_macro!(attributes____dynatrace_entity_process_id, Utf8, true, Option<String>);
        $field_macro!(attributes____elastic_node_name, Utf8, true, Option<String>);
        $field_macro!(attributes____istio_mesh_id, Utf8, true, Option<String>);
        $field_macro!(attributes____cloudfoundry_application_id, Utf8, true, Option<String>);
        $field_macro!(attributes____cloudfoundry_space_id, Utf8, true, Option<String>);
        $field_macro!(attributes____opentelemetry_collector_name, Utf8, true, Option<String>);
        $field_macro!(attributes____instrumentation_name, Utf8, true, Option<String>);
        $field_macro!(attributes____instrumentation_version, Utf8, true, Option<String>);
        $field_macro!(attributes____log_source, Utf8, true, Option<String>);
        $field_macro!(events, Utf8, true, Option<String>);
        $field_macro!(links, Utf8, true, Option<String>);
        $field_macro!(status_code, Utf8, true, Option<String>);
        $field_macro!(status_message, Utf8, true, Option<String>);
        $field_macro!(instrumentation_library_name, Utf8, true, Option<String>);
        $field_macro!(instrumentation_library_version, Utf8, true, Option<String>);
    };
}

pub(crate) use define_telemetry_fields;