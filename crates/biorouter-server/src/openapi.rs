use biorouter::agents::extension::Envs;
use biorouter::agents::extension::ToolInfo;
use biorouter::agents::ExtensionConfig;
use biorouter::agents::ReasoningEffort;
use biorouter::config::permission::PermissionLevel;
use biorouter::config::ExtensionEntry;
use biorouter::conversation::Conversation;
use biorouter::model::ModelConfig;
use biorouter::permission::permission_confirmation::PrincipalType;
use biorouter::providers::base::{ConfigKey, ModelInfo, ProviderMetadata, ProviderType};
use biorouter::session::{
    ActivityWindow, DailyActivity, Session, SessionInsights, SessionType, SystemInfo,
};
use rmcp::model::{
    Annotations, Content, EmbeddedResource, Icon, ImageContent, JsonObject, RawAudioContent,
    RawEmbeddedResource, RawImageContent, RawResource, RawTextContent, ResourceContents, Role,
    TextContent, Tool, ToolAnnotations,
};
use utoipa::{OpenApi, ToSchema};

use biorouter::config::declarative_providers::{
    DeclarativeProviderConfig, LoadedProvider, ProviderEngine,
};
use biorouter::conversation::message::{
    ActionRequired, ActionRequiredData, FrontendToolRequest, Message, MessageContent,
    MessageMetadata, RedactedThinkingContent, SystemNotificationContent, SystemNotificationType,
    ThinkingContent, TokenState, ToolConfirmationRequest, ToolRequest, ToolResponse,
};
use biorouter::conversation::tool_preview::{ToolPreview, ToolPreviewLine, ToolPreviewLineKind};
use biorouter::permission::tool_risk::ToolRisk;

use crate::routes::reply::MessageEvent;
use crate::routes::workflow_utils::WorkflowManifest;
use utoipa::openapi::schema::{
    AdditionalProperties, AnyOfBuilder, ArrayBuilder, ObjectBuilder, OneOfBuilder, Schema,
    SchemaFormat, SchemaType,
};
use utoipa::openapi::{AllOfBuilder, Ref, RefOr};

macro_rules! derive_utoipa {
    ($inner_type:ident as $schema_name:ident) => {
        struct $schema_name {}

        impl<'__s> ToSchema<'__s> for $schema_name {
            fn schema() -> (&'__s str, utoipa::openapi::RefOr<utoipa::openapi::Schema>) {
                let settings = rmcp::schemars::generate::SchemaSettings::openapi3();
                let generator = settings.into_generator();
                let schema = generator.into_root_schema_for::<$inner_type>();
                let schema = convert_schemars_to_utoipa(schema);
                (stringify!($inner_type), schema)
            }

            fn aliases() -> Vec<(&'__s str, utoipa::openapi::schema::Schema)> {
                Vec::new()
            }
        }
    };
}

fn convert_schemars_to_utoipa(schema: rmcp::schemars::Schema) -> RefOr<Schema> {
    if let Some(true) = schema.as_bool() {
        return RefOr::T(Schema::Object(ObjectBuilder::new().build()));
    }

    if let Some(false) = schema.as_bool() {
        return RefOr::T(Schema::Object(ObjectBuilder::new().build()));
    }

    if let Some(obj) = schema.as_object() {
        return convert_json_object_to_utoipa(obj);
    }

    RefOr::T(Schema::Object(ObjectBuilder::new().build()))
}

fn convert_json_object_to_utoipa(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> RefOr<Schema> {
    use serde_json::Value;

    if let Some(Value::String(reference)) = obj.get("$ref") {
        return RefOr::Ref(Ref::new(reference.clone()));
    }

    if let Some(Value::Array(one_of)) = obj.get("oneOf") {
        let mut builder = OneOfBuilder::new();
        for item in one_of {
            if let Ok(schema) = rmcp::schemars::Schema::try_from(item.clone()) {
                builder = builder.item(convert_schemars_to_utoipa(schema));
            }
        }
        return RefOr::T(Schema::OneOf(builder.build()));
    }

    if let Some(Value::Array(all_of)) = obj.get("allOf") {
        let mut builder = AllOfBuilder::new();
        for item in all_of {
            if let Ok(schema) = rmcp::schemars::Schema::try_from(item.clone()) {
                builder = builder.item(convert_schemars_to_utoipa(schema));
            }
        }
        return RefOr::T(Schema::AllOf(builder.build()));
    }

    if let Some(Value::Array(any_of)) = obj.get("anyOf") {
        let mut builder = AnyOfBuilder::new();
        for item in any_of {
            if let Ok(schema) = rmcp::schemars::Schema::try_from(item.clone()) {
                builder = builder.item(convert_schemars_to_utoipa(schema));
            }
        }
        return RefOr::T(Schema::AnyOf(builder.build()));
    }

    match obj.get("type") {
        Some(Value::String(type_str)) => convert_typed_schema(type_str, obj),
        Some(Value::Array(types)) => {
            let mut builder = AnyOfBuilder::new();
            for type_val in types {
                if let Value::String(type_str) = type_val {
                    builder = builder.item(convert_typed_schema(type_str, obj));
                }
            }
            RefOr::T(Schema::AnyOf(builder.build()))
        }
        None => RefOr::T(Schema::Object(ObjectBuilder::new().build())),
        _ => RefOr::T(Schema::Object(ObjectBuilder::new().build())),
    }
}

#[allow(clippy::too_many_lines)]
fn convert_typed_schema(
    type_str: &str,
    obj: &serde_json::Map<String, serde_json::Value>,
) -> RefOr<Schema> {
    use serde_json::Value;

    match type_str {
        "object" => {
            let mut object_builder = ObjectBuilder::new();

            if let Some(Value::Object(properties)) = obj.get("properties") {
                for (name, prop_value) in properties {
                    if let Ok(prop_schema) = rmcp::schemars::Schema::try_from(prop_value.clone()) {
                        let prop = convert_schemars_to_utoipa(prop_schema);
                        object_builder = object_builder.property(name, prop);
                    }
                }
            }

            if let Some(Value::Array(required)) = obj.get("required") {
                for req in required {
                    if let Value::String(field_name) = req {
                        object_builder = object_builder.required(field_name);
                    }
                }
            }

            if let Some(additional) = obj.get("additionalProperties") {
                match additional {
                    Value::Bool(false) => {
                        object_builder = object_builder
                            .additional_properties(Some(AdditionalProperties::FreeForm(false)));
                    }
                    Value::Bool(true) => {
                        object_builder = object_builder
                            .additional_properties(Some(AdditionalProperties::FreeForm(true)));
                    }
                    _ => {
                        if let Ok(schema) = rmcp::schemars::Schema::try_from(additional.clone()) {
                            let schema = convert_schemars_to_utoipa(schema);
                            object_builder = object_builder
                                .additional_properties(Some(AdditionalProperties::RefOr(schema)));
                        }
                    }
                }
            }

            RefOr::T(Schema::Object(object_builder.build()))
        }
        "array" => {
            let mut array_builder = ArrayBuilder::new();

            if let Some(items) = obj.get("items") {
                match items {
                    Value::Object(_) | Value::Bool(_) => {
                        if let Ok(item_schema) = rmcp::schemars::Schema::try_from(items.clone()) {
                            let item_schema = convert_schemars_to_utoipa(item_schema);
                            array_builder = array_builder.items(item_schema);
                        }
                    }
                    Value::Array(item_schemas) => {
                        let mut any_of = AnyOfBuilder::new();
                        for item in item_schemas {
                            if let Ok(schema) = rmcp::schemars::Schema::try_from(item.clone()) {
                                any_of = any_of.item(convert_schemars_to_utoipa(schema));
                            }
                        }
                        let any_of_schema = RefOr::T(Schema::AnyOf(any_of.build()));
                        array_builder = array_builder.items(any_of_schema);
                    }
                    _ => {}
                }
            }

            if let Some(Value::Number(min_items)) = obj.get("minItems") {
                if let Some(min) = min_items.as_u64() {
                    array_builder = array_builder.min_items(Some(min as usize));
                }
            }
            if let Some(Value::Number(max_items)) = obj.get("maxItems") {
                if let Some(max) = max_items.as_u64() {
                    array_builder = array_builder.max_items(Some(max as usize));
                }
            }

            RefOr::T(Schema::Array(array_builder.build()))
        }
        "string" => {
            let mut object_builder = ObjectBuilder::new().schema_type(SchemaType::String);

            if let Some(Value::Number(min_length)) = obj.get("minLength") {
                if let Some(min) = min_length.as_u64() {
                    object_builder = object_builder.min_length(Some(min as usize));
                }
            }
            if let Some(Value::Number(max_length)) = obj.get("maxLength") {
                if let Some(max) = max_length.as_u64() {
                    object_builder = object_builder.max_length(Some(max as usize));
                }
            }
            if let Some(Value::String(pattern)) = obj.get("pattern") {
                object_builder = object_builder.pattern(Some(pattern.clone()));
            }
            if let Some(Value::String(format)) = obj.get("format") {
                object_builder = object_builder.format(Some(SchemaFormat::Custom(format.clone())));
            }

            RefOr::T(Schema::Object(object_builder.build()))
        }
        "number" => {
            let mut object_builder = ObjectBuilder::new().schema_type(SchemaType::Number);

            if let Some(Value::Number(minimum)) = obj.get("minimum") {
                if let Some(min) = minimum.as_f64() {
                    object_builder = object_builder.minimum(Some(min));
                }
            }
            if let Some(Value::Number(maximum)) = obj.get("maximum") {
                if let Some(max) = maximum.as_f64() {
                    object_builder = object_builder.maximum(Some(max));
                }
            }
            if let Some(Value::Number(exclusive_minimum)) = obj.get("exclusiveMinimum") {
                if let Some(min) = exclusive_minimum.as_f64() {
                    object_builder = object_builder.exclusive_minimum(Some(min));
                }
            }
            if let Some(Value::Number(exclusive_maximum)) = obj.get("exclusiveMaximum") {
                if let Some(max) = exclusive_maximum.as_f64() {
                    object_builder = object_builder.exclusive_maximum(Some(max));
                }
            }
            if let Some(Value::Number(multiple_of)) = obj.get("multipleOf") {
                if let Some(mult) = multiple_of.as_f64() {
                    object_builder = object_builder.multiple_of(Some(mult));
                }
            }

            RefOr::T(Schema::Object(object_builder.build()))
        }
        "integer" => {
            let mut object_builder = ObjectBuilder::new().schema_type(SchemaType::Integer);

            if let Some(Value::Number(minimum)) = obj.get("minimum") {
                if let Some(min) = minimum.as_f64() {
                    object_builder = object_builder.minimum(Some(min));
                }
            }
            if let Some(Value::Number(maximum)) = obj.get("maximum") {
                if let Some(max) = maximum.as_f64() {
                    object_builder = object_builder.maximum(Some(max));
                }
            }
            if let Some(Value::Number(exclusive_minimum)) = obj.get("exclusiveMinimum") {
                if let Some(min) = exclusive_minimum.as_f64() {
                    object_builder = object_builder.exclusive_minimum(Some(min));
                }
            }
            if let Some(Value::Number(exclusive_maximum)) = obj.get("exclusiveMaximum") {
                if let Some(max) = exclusive_maximum.as_f64() {
                    object_builder = object_builder.exclusive_maximum(Some(max));
                }
            }
            if let Some(Value::Number(multiple_of)) = obj.get("multipleOf") {
                if let Some(mult) = multiple_of.as_f64() {
                    object_builder = object_builder.multiple_of(Some(mult));
                }
            }

            RefOr::T(Schema::Object(object_builder.build()))
        }
        "boolean" => RefOr::T(Schema::Object(
            ObjectBuilder::new()
                .schema_type(SchemaType::Boolean)
                .build(),
        )),
        "null" => RefOr::T(Schema::Object(
            ObjectBuilder::new().schema_type(SchemaType::String).build(),
        )),
        _ => RefOr::T(Schema::Object(ObjectBuilder::new().build())),
    }
}

derive_utoipa!(Role as RoleSchema);
derive_utoipa!(Content as ContentSchema);
derive_utoipa!(EmbeddedResource as EmbeddedResourceSchema);
derive_utoipa!(ImageContent as ImageContentSchema);
derive_utoipa!(TextContent as TextContentSchema);
derive_utoipa!(RawTextContent as RawTextContentSchema);
derive_utoipa!(RawImageContent as RawImageContentSchema);
derive_utoipa!(RawAudioContent as RawAudioContentSchema);
derive_utoipa!(RawEmbeddedResource as RawEmbeddedResourceSchema);
derive_utoipa!(RawResource as RawResourceSchema);
derive_utoipa!(Tool as ToolSchema);
derive_utoipa!(ToolAnnotations as ToolAnnotationsSchema);
derive_utoipa!(Annotations as AnnotationsSchema);
derive_utoipa!(ResourceContents as ResourceContentsSchema);
derive_utoipa!(JsonObject as JsonObjectSchema);
derive_utoipa!(Icon as IconSchema);

#[derive(OpenApi)]
#[openapi(
    paths(
        super::routes::status::status,
        super::routes::status::system_info,
        super::routes::status::diagnostics,
        super::routes::active_work::list_active_work,
        super::routes::active_work::cancel_active_work,
        super::routes::mcp_ui_proxy::mcp_ui_proxy,
        super::routes::config_management::backup_config,
        super::routes::config_management::detect_provider,
        super::routes::config_management::get_detectable_providers,
        super::routes::config_management::recover_config,
        super::routes::config_management::validate_config,
        super::routes::config_management::init_config,
        super::routes::config_management::upsert_config,
        super::routes::config_management::remove_config,
        super::routes::config_management::read_config,
        super::routes::config_management::add_extension,
        super::routes::config_management::remove_extension,
        super::routes::config_management::get_extensions,
        super::routes::config_management::read_all_config,
        super::routes::config_management::providers,
        super::routes::config_management::get_provider_models,
        super::routes::config_management::get_slash_commands,
        super::routes::config_management::upsert_permissions,
        super::routes::config_management::create_custom_provider,
        super::routes::config_management::get_custom_provider,
        super::routes::config_management::update_custom_provider,
        super::routes::config_management::remove_custom_provider,
        super::routes::config_management::check_provider,
        super::routes::config_management::set_config_provider,
        super::routes::config_management::get_pricing,
        super::routes::agent::start_agent,
        super::routes::agent::resume_agent,
        super::routes::agent::stop_agent,
        super::routes::agent::restart_agent,
        super::routes::agent::update_working_dir,
        super::routes::agent::get_tools,
        super::routes::agent::read_resource,
        super::routes::agent::call_tool,
        super::routes::agent::list_apps,
        super::routes::agent::update_from_session,
        super::routes::agent::agent_add_extension,
        super::routes::agent::agent_remove_extension,
        super::routes::agent::update_agent_provider,
        super::routes::action_required::confirm_tool_action,
        super::routes::reply::reply,
        super::routes::reply::interrupt,
        super::routes::reply::cancel_turn,
        super::routes::session::list_sessions,
        super::routes::session::get_session,
        super::routes::session::get_session_insights,
        super::routes::session::get_session_activity,
        super::routes::session::update_session_name,
        super::routes::session::delete_session,
        super::routes::session::export_session,
        super::routes::session::import_session,
        super::routes::session::update_session_user_workflow_values,
        super::routes::session::edit_message,
        super::routes::session::diverge_session,
        super::routes::session::get_session_extensions,
        super::routes::schedule::create_schedule,
        super::routes::schedule::list_schedules,
        super::routes::schedule::delete_schedule,
        super::routes::schedule::update_schedule,
        super::routes::schedule::run_now_handler,
        super::routes::schedule::pause_schedule,
        super::routes::schedule::unpause_schedule,
        super::routes::schedule::kill_running_job,
        super::routes::schedule::inspect_running_job,
        super::routes::schedule::sessions_handler,
        super::routes::workflow::create_workflow,
        super::routes::workflow::encode_workflow,
        super::routes::workflow::decode_workflow,
        super::routes::workflow::scan_workflow,
        super::routes::workflow::list_workflows,
        super::routes::workflow::delete_workflow,
        super::routes::workflow::schedule_workflow,
        super::routes::workflow::set_workflow_slash_command,
        super::routes::workflow::save_workflow,
        super::routes::workflow::parse_workflow,
        super::routes::workflow::workflow_to_yaml,
        super::routes::setup::start_openrouter_setup,
        super::routes::setup::start_tetrate_setup,
        super::routes::llamacpp::llamacpp_status,
        super::routes::llamacpp::llamacpp_ensure,
        super::routes::llamacpp::llamacpp_warmup,
        super::routes::llamacpp::llamacpp_delete,
        super::routes::llamacpp::llamacpp_stop,
        super::routes::tunnel::start_tunnel,
        super::routes::tunnel::stop_tunnel,
        super::routes::tunnel::get_tunnel_status,
        super::routes::knowledge::list_bases,
        super::routes::knowledge::create_base,
        super::routes::knowledge::get_base,
        super::routes::knowledge::set_default_model,
        super::routes::knowledge::delete_base,
        super::routes::knowledge::get_graph,
        super::routes::knowledge::get_location,
        super::routes::knowledge::list_pages,
        super::routes::knowledge::read_page,
        super::routes::knowledge::get_page_body,
        super::routes::knowledge::write_page,
        super::routes::knowledge::list_history,
        super::routes::knowledge::preview_state,
        super::routes::knowledge::restore_state,
        super::routes::knowledge::add_raw_source,
        super::routes::knowledge::ingest,
        super::routes::knowledge::ingest_conversation,
        super::routes::knowledge::query_kb,
        super::routes::knowledge::lint,
        super::routes::knowledge::export_brkb,
        super::routes::knowledge::import_brkb,
        super::routes::knowledge::reclassify,
        super::routes::knowledge::override_credibility,
        super::routes::knowledge::check_model,
        super::routes::knowledge::get_active,
        super::routes::knowledge::set_active,
    ),
    components(schemas(
        super::routes::config_management::UpsertConfigQuery,
        super::routes::config_management::ConfigKeyQuery,
        super::routes::config_management::DetectProviderRequest,
        super::routes::config_management::DetectProviderResponse,
        super::routes::config_management::DetectableProvider,
        super::routes::config_management::DetectableProvidersResponse,
        super::routes::config_management::ConfigResponse,
        super::routes::config_management::ProvidersResponse,
        super::routes::config_management::ProviderDetails,
        super::routes::config_management::SlashCommandsResponse,
        super::routes::config_management::SlashCommand,
        super::routes::config_management::CommandType,
        super::routes::config_management::ExtensionResponse,
        super::routes::config_management::ExtensionQuery,
        super::routes::config_management::ToolPermission,
        super::routes::config_management::UpsertPermissionsQuery,
        super::routes::config_management::UpdateCustomProviderRequest,
        super::routes::config_management::CheckProviderRequest,
        super::routes::config_management::SetProviderRequest,
        super::routes::config_management::PricingQuery,
        super::routes::config_management::PricingResponse,
        super::routes::config_management::PricingData,
        super::routes::action_required::ConfirmToolActionRequest,
        super::routes::reply::ChatRequest,
        super::routes::reply::InterruptRequest,
        super::routes::reply::CancelTurnRequest,
        super::routes::reply::CancelTurnResponse,
        super::routes::session::ImportSessionRequest,
        super::routes::session::SessionListResponse,
        super::routes::session::UpdateSessionNameRequest,
        super::routes::session::UpdateSessionUserWorkflowValuesRequest,
        super::routes::session::UpdateSessionUserWorkflowValuesResponse,
        super::routes::session::EditType,
        super::routes::session::EditMessageRequest,
        super::routes::session::EditMessageResponse,
        super::routes::session::DivergeSessionRequest,
        super::routes::session::DivergeSessionResponse,
        super::routes::session::SessionExtensionsResponse,
        Message,
        MessageContent,
        MessageMetadata,
        TokenState,
        ContentSchema,
        EmbeddedResourceSchema,
        ImageContentSchema,
        AnnotationsSchema,
        TextContentSchema,
        RawTextContentSchema,
        RawImageContentSchema,
        RawAudioContentSchema,
        RawEmbeddedResourceSchema,
        RawResourceSchema,
        ToolResponse,
        ToolRequest,
        ToolConfirmationRequest,
        ActionRequired,
        ActionRequiredData,
        ToolPreview,
        ToolPreviewLine,
        ToolPreviewLineKind,
        ToolRisk,
        ThinkingContent,
        RedactedThinkingContent,
        FrontendToolRequest,
        ResourceContentsSchema,
        SystemNotificationType,
        SystemNotificationContent,
        MessageEvent,
        JsonObjectSchema,
        RoleSchema,
        ProviderMetadata,
        ProviderType,
        LoadedProvider,
        ProviderEngine,
        DeclarativeProviderConfig,
        ExtensionEntry,
        ExtensionConfig,
        ConfigKey,
        Envs,
        WorkflowManifest,
        ToolSchema,
        ToolAnnotationsSchema,
        ToolInfo,
        PermissionLevel,
        PrincipalType,
        ModelInfo,
        ModelConfig,
        ReasoningEffort,
        Session,
        SessionInsights,
        ActivityWindow,
        DailyActivity,
        SessionType,
        SystemInfo,
        Conversation,
        IconSchema,
        biorouter::session::extension_data::ExtensionData,
        super::routes::schedule::CreateScheduleRequest,
        super::routes::schedule::UpdateScheduleRequest,
        super::routes::schedule::KillJobResponse,
        super::routes::schedule::InspectJobResponse,
        biorouter::scheduler::ScheduledJob,
        super::routes::active_work::ActiveWorkItemDto,
        super::routes::active_work::ActiveWorkResponse,
        super::routes::active_work::CancelActiveWorkResponse,
        super::routes::schedule::RunNowResponse,
        super::routes::schedule::ListSchedulesResponse,
        super::routes::schedule::SessionsQuery,
        super::routes::schedule::SessionDisplayInfo,
        super::routes::workflow::CreateWorkflowRequest,
        super::routes::workflow::AuthorRequest,
        super::routes::workflow::CreateWorkflowResponse,
        super::routes::workflow::EncodeWorkflowRequest,
        super::routes::workflow::EncodeWorkflowResponse,
        super::routes::workflow::DecodeWorkflowRequest,
        super::routes::workflow::DecodeWorkflowResponse,
        super::routes::workflow::ScanWorkflowRequest,
        super::routes::workflow::ScanWorkflowResponse,
        super::routes::workflow::ListWorkflowResponse,
        super::routes::workflow::ScheduleWorkflowRequest,
        super::routes::workflow::SetSlashCommandRequest,
        super::routes::workflow::DeleteWorkflowRequest,
        super::routes::workflow::SaveWorkflowRequest,
        super::routes::workflow::SaveWorkflowResponse,
        super::routes::errors::ErrorResponse,
        super::routes::workflow::ParseWorkflowRequest,
        super::routes::workflow::ParseWorkflowResponse,
        super::routes::workflow::WorkflowToYamlRequest,
        super::routes::workflow::WorkflowToYamlResponse,
        biorouter::workflow::Workflow,
        biorouter::workflow::Author,
        biorouter::workflow::Settings,
        biorouter::workflow::WorkflowKnowledgeBases,
        biorouter::workflow::WorkflowParameter,
        biorouter::workflow::WorkflowParameterInputType,
        biorouter::workflow::WorkflowParameterRequirement,
        biorouter::workflow::Response,
        biorouter::workflow::SubWorkflow,
        biorouter::agents::types::RetryConfig,
        biorouter::agents::types::SuccessCheck,
        super::routes::agent::UpdateProviderRequest,
        super::routes::agent::GetToolsQuery,
        super::routes::agent::ReadResourceRequest,
        super::routes::agent::ReadResourceResponse,
        super::routes::agent::CallToolRequest,
        super::routes::agent::CallToolResponse,
        super::routes::agent::ListAppsRequest,
        super::routes::agent::ListAppsResponse,
        super::routes::agent::StartAgentRequest,
        super::routes::agent::ResumeAgentRequest,
        super::routes::agent::StopAgentRequest,
        super::routes::agent::RestartAgentRequest,
        super::routes::agent::UpdateWorkingDirRequest,
        super::routes::agent::UpdateFromSessionRequest,
        super::routes::agent::AddExtensionRequest,
        super::routes::agent::RemoveExtensionRequest,
        super::routes::agent::ResumeAgentResponse,
        super::routes::agent::RestartAgentResponse,
        biorouter::agents::ExtensionLoadResult,
        super::routes::setup::SetupResponse,
        super::routes::llamacpp::LlamaCppModel,
        super::routes::llamacpp::LlamaCppSuitability,
        super::routes::llamacpp::LlamaCppStatusResponse,
        super::routes::llamacpp::LlamaCppSystemInfo,
        super::routes::llamacpp::LlamaCppEnsureRequest,
        super::routes::llamacpp::LlamaCppWarmupRequest,
        super::routes::llamacpp::LlamaCppWarmupResponse,
        super::routes::llamacpp::LlamaCppDeleteRequest,
        super::routes::llamacpp::LlamaCppDeleteResponse,
        biorouter::providers::llamacpp_sidecar::ModelCacheStatus,
        biorouter::providers::llamacpp_sidecar::SidecarStatus,
        biorouter::providers::llamacpp_sidecar::SidecarState,
        super::tunnel::TunnelInfo,
        super::tunnel::TunnelState,
        biorouter::biorouter_apps::BioRouterApp,
        biorouter::biorouter_apps::WindowProps,
        biorouter::biorouter_apps::McpAppResource,
        biorouter::biorouter_apps::CspMetadata,
        biorouter::biorouter_apps::UiMetadata,
        biorouter::biorouter_apps::ResourceMetadata,
        // knowledge types
        biorouter_mcp::knowledge::types::Manifest,
        biorouter_mcp::knowledge::types::Graph,
        biorouter_mcp::knowledge::types::GraphNode,
        biorouter_mcp::knowledge::types::GraphEdge,
        biorouter_mcp::knowledge::types::HistoryEntry,
        biorouter_mcp::knowledge::types::ChangeKind,
        biorouter_mcp::knowledge::types::Credibility,
        biorouter_mcp::knowledge::types::CredibilityTier,
        biorouter_mcp::knowledge::types::ModelRef,
        biorouter_mcp::knowledge::types::SourceMeta,
        biorouter_mcp::knowledge::types::PageKind,
        biorouter_mcp::knowledge::store::PageRef,
        biorouter_mcp::knowledge::store::PageContent,
        // knowledge route DTOs
        super::routes::knowledge::CreateBaseBody,
        super::routes::knowledge::SetDefaultModelBody,
        super::routes::knowledge::ListPagesQuery,
        super::routes::knowledge::ReadPageQuery,
        super::routes::knowledge::ReadPageResponse,
        super::routes::knowledge::LocationResponse,
        super::routes::knowledge::IngestConversationBody,
        super::routes::knowledge::WritePageBody,
        super::routes::knowledge::CommitResponse,
        super::routes::knowledge::HistoryQuery,
        super::routes::knowledge::PreviewBody,
        super::routes::knowledge::PreviewResponse,
        super::routes::knowledge::RestoreBody,
        super::routes::knowledge::RestoreResponse,
        super::routes::knowledge::RawSourceResponse,
        super::routes::knowledge::CredibilityResponse,
        super::routes::knowledge::IngestBody,
        super::routes::knowledge::QueryBody,
        super::routes::knowledge::LintBody,
        super::routes::knowledge::CheckModelBody,
        super::routes::knowledge::CheckModelResponse,
        super::routes::knowledge::SetActiveBody,
        super::routes::knowledge::ActiveKbResponse,
    ))
)]
pub struct ApiDoc;

#[allow(dead_code)] // Used by generate_schema binary
pub fn generate_schema() -> String {
    let api_doc = ApiDoc::openapi();
    serde_json::to_string_pretty(&api_doc).unwrap()
}
