import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  CleanupJenkinsArtifactInput,
  CreateJenkinsArtifactDeploymentCandidateInput,
  CreateJenkinsBuildDeploymentDryRunInput,
  DeleteJenkinsParameterTemplateInput,
  DeploymentCandidate,
  DeploymentPlan,
  DownloadJenkinsArtifactInput,
  ExecuteJenkinsBuildApprovedInput,
  ExecuteJenkinsBuildStopApprovedInput,
  ForgetJenkinsRecentParameterValueInput,
  GenerateJenkinsFailureAnalysisInput,
  GetJenkinsBuildInput,
  GetJenkinsJobDetailInput,
  InspectJenkinsFileParameterInput,
  JenkinsArtifact,
  JenkinsBuild,
  JenkinsBuildAnalysis,
  JenkinsBuildLogInput,
  JenkinsBuildLogResult,
  JenkinsBuildStopResult,
  JenkinsBuildTriggerResult,
  JenkinsConnection,
  JenkinsConnectionTestResult,
  JenkinsFileParameterMetadata,
  JenkinsJob,
  JenkinsJobDetail,
  JenkinsParameterDefinitionsResult,
  JenkinsParameterTemplate,
  JenkinsQueueItem,
  JenkinsRecentParameterValue,
  ListJenkinsArtifactsInput,
  ListJenkinsBuildsInput,
  ListJenkinsConnectionsInput,
  ListJenkinsJobsInput,
  ListJenkinsParameterTemplatesInput,
  ListJenkinsParametersInput,
  ListJenkinsRecentParameterValuesInput,
  PollJenkinsQueueItemInput,
  RecordJenkinsLogCopyAuditInput,
  SetJenkinsJobFavoriteInput,
  StopJenkinsBuildInput,
  TriggerJenkinsBuildInput,
  UpsertJenkinsConnectionInput,
  UpsertJenkinsParameterTemplateInput,
  VerifyJenkinsParameterDefinitionHashInput,
  ApprovalRequest,
} from "@/types";

export const jenkinsApi = {
  listConnections: (input: ListJenkinsConnectionsInput = {}) =>
    hasTauriRuntime()
      ? invoke<JenkinsConnection[]>("list_jenkins_connections", { input })
      : devApiFetch<JenkinsConnection[]>("/jenkins/connections/list", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  upsertConnection: (input: UpsertJenkinsConnectionInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsConnection>("upsert_jenkins_connection", { input })
      : devApiFetch<JenkinsConnection>("/jenkins/connections", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  deleteConnection: (connectionKey: string) =>
    hasTauriRuntime()
      ? invoke<void>("delete_jenkins_connection", { connectionKey })
      : devApiFetch<void>(
          `/jenkins/connections/${encodeURIComponent(connectionKey)}`,
          {
            method: "DELETE",
          },
        ),
  restoreConnection: (connectionKey: string) =>
    hasTauriRuntime()
      ? invoke<JenkinsConnection>("restore_jenkins_connection", {
          connectionKey,
        })
      : devApiFetch<JenkinsConnection>(
          `/jenkins/connections/${encodeURIComponent(connectionKey)}/restore`,
          { method: "POST" },
        ),
  duplicateConnection: (connectionKey: string) =>
    hasTauriRuntime()
      ? invoke<JenkinsConnection>("duplicate_jenkins_connection", {
          connectionKey,
        })
      : devApiFetch<JenkinsConnection>(
          `/jenkins/connections/${encodeURIComponent(connectionKey)}/duplicate`,
          { method: "POST" },
        ),
  testConnection: (connectionKey: string) =>
    hasTauriRuntime()
      ? invoke<JenkinsConnectionTestResult>("test_jenkins_connection", {
          connectionKey,
        })
      : devApiFetch<JenkinsConnectionTestResult>(
          `/jenkins/connections/${encodeURIComponent(connectionKey)}/test`,
          { method: "POST" },
        ),
  listJobs: (input: ListJenkinsJobsInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsJob[]>("list_jenkins_jobs", { input })
      : devApiFetch<JenkinsJob[]>("/jenkins/jobs/list", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  getJobDetail: (input: GetJenkinsJobDetailInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsJobDetail>("get_jenkins_job_detail", { input })
      : devApiFetch<JenkinsJobDetail>("/jenkins/jobs/detail", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  setJobFavorite: (input: SetJenkinsJobFavoriteInput) =>
    hasTauriRuntime()
      ? invoke<boolean>("set_jenkins_job_favorite", { input })
      : devApiFetch<boolean>("/jenkins/jobs/favorite", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  listParameters: (input: ListJenkinsParametersInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsParameterDefinitionsResult>("list_jenkins_parameters", {
          input,
        })
      : devApiFetch<JenkinsParameterDefinitionsResult>(
          "/jenkins/parameters/list",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  listRecentParameterValues: (input: ListJenkinsRecentParameterValuesInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsRecentParameterValue[]>(
          "list_jenkins_recent_parameter_values",
          { input },
        )
      : devApiFetch<JenkinsRecentParameterValue[]>(
          "/jenkins/parameters/recent/list",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  forgetRecentParameterValue: (
    input: ForgetJenkinsRecentParameterValueInput,
  ) =>
    hasTauriRuntime()
      ? invoke<boolean>("forget_jenkins_recent_parameter_value", { input })
      : devApiFetch<boolean>("/jenkins/parameters/recent/forget", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  listParameterTemplates: (input: ListJenkinsParameterTemplatesInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsParameterTemplate[]>("list_jenkins_parameter_templates", {
          input,
        })
      : devApiFetch<JenkinsParameterTemplate[]>(
          "/jenkins/parameters/templates/list",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  upsertParameterTemplate: (input: UpsertJenkinsParameterTemplateInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsParameterTemplate>("upsert_jenkins_parameter_template", {
          input,
        })
      : devApiFetch<JenkinsParameterTemplate>(
          "/jenkins/parameters/templates/upsert",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  deleteParameterTemplate: (input: DeleteJenkinsParameterTemplateInput) =>
    hasTauriRuntime()
      ? invoke<boolean>("delete_jenkins_parameter_template", { input })
      : devApiFetch<boolean>("/jenkins/parameters/templates/delete", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  verifyParameterDefinitionHash: (
    input: VerifyJenkinsParameterDefinitionHashInput,
  ) =>
    hasTauriRuntime()
      ? invoke<JenkinsParameterDefinitionsResult>(
          "verify_jenkins_parameter_definition_hash",
          {
            input,
          },
        )
      : devApiFetch<JenkinsParameterDefinitionsResult>(
          "/jenkins/parameters/verify-hash",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  inspectFileParameter: (input: InspectJenkinsFileParameterInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsFileParameterMetadata>("inspect_jenkins_file_parameter", {
          input,
        })
      : devApiFetch<JenkinsFileParameterMetadata>(
          "/jenkins/parameters/file/inspect",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  createTriggerApproval: (input: TriggerJenkinsBuildInput) =>
    hasTauriRuntime()
      ? invoke<ApprovalRequest>("create_jenkins_build_trigger_approval", {
          input,
        })
      : devApiFetch<ApprovalRequest>("/jenkins/builds/trigger-approval", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  executeTriggerApproved: (input: ExecuteJenkinsBuildApprovedInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsBuildTriggerResult>(
          "execute_jenkins_build_trigger_approved",
          { input },
        )
      : devApiFetch<JenkinsBuildTriggerResult>(
          "/jenkins/builds/trigger-approved",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  triggerWithoutApproval: (input: TriggerJenkinsBuildInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsBuildTriggerResult>(
          "trigger_jenkins_build_without_approval",
          { input },
        )
      : devApiFetch<JenkinsBuildTriggerResult>(
          "/jenkins/builds/trigger-without-approval",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  createStopApproval: (input: StopJenkinsBuildInput) =>
    hasTauriRuntime()
      ? invoke<ApprovalRequest>("create_jenkins_build_stop_approval", { input })
      : devApiFetch<ApprovalRequest>("/jenkins/builds/stop-approval", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  executeStopApproved: (input: ExecuteJenkinsBuildStopApprovedInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsBuildStopResult>("execute_jenkins_build_stop_approved", {
          input,
        })
      : devApiFetch<JenkinsBuildStopResult>("/jenkins/builds/stop-approved", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  stopWithoutApproval: (input: StopJenkinsBuildInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsBuildStopResult>("stop_jenkins_build_without_approval", {
          input,
        })
      : devApiFetch<JenkinsBuildStopResult>(
          "/jenkins/builds/stop-without-approval",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  listBuilds: (input: ListJenkinsBuildsInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsBuild[]>("list_jenkins_builds", { input })
      : devApiFetch<JenkinsBuild[]>("/jenkins/builds/list", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  syncUnfinishedRuns: (connectionKey: string) =>
    hasTauriRuntime()
      ? invoke<JenkinsBuild[]>("sync_unfinished_jenkins_runs", {
          connectionKey,
        })
      : devApiFetch<JenkinsBuild[]>("/jenkins/builds/sync-unfinished", {
          method: "POST",
          body: JSON.stringify({ connectionKey }),
        }),
  getBuildDetail: (input: GetJenkinsBuildInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsBuild>("get_jenkins_build_detail", { input })
      : devApiFetch<JenkinsBuild>("/jenkins/builds/detail", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  readBuildLog: (input: JenkinsBuildLogInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsBuildLogResult>("read_jenkins_build_log", { input })
      : devApiFetch<JenkinsBuildLogResult>("/jenkins/builds/log", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  recordLogCopyAudit: (input: RecordJenkinsLogCopyAuditInput) =>
    hasTauriRuntime()
      ? invoke<void>("record_jenkins_log_copy_audit", { input })
      : devApiFetch<void>("/jenkins/builds/log/copy-audit", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  generateFailureAnalysis: (input: GenerateJenkinsFailureAnalysisInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsBuildAnalysis>("generate_jenkins_failure_analysis", {
          input,
        })
      : devApiFetch<JenkinsBuildAnalysis>("/jenkins/builds/failure-analysis", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  getLatestBuildAnalysis: (input: GetJenkinsBuildInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsBuildAnalysis | null>(
          "get_latest_jenkins_build_analysis",
          { input },
        )
      : devApiFetch<JenkinsBuildAnalysis | null>(
          "/jenkins/builds/failure-analysis/latest",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  listArtifacts: (input: ListJenkinsArtifactsInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsArtifact[]>("list_jenkins_artifacts", { input })
      : devApiFetch<JenkinsArtifact[]>("/jenkins/artifacts/list", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  downloadArtifact: (input: DownloadJenkinsArtifactInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsArtifact>("download_jenkins_artifact", { input })
      : devApiFetch<JenkinsArtifact>("/jenkins/artifacts/download", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  cleanupArtifactLocalFile: (input: CleanupJenkinsArtifactInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsArtifact>("cleanup_jenkins_artifact_local_file", {
          input,
        })
      : devApiFetch<JenkinsArtifact>("/jenkins/artifacts/cleanup", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  createArtifactDeploymentCandidate: (
    input: CreateJenkinsArtifactDeploymentCandidateInput,
  ) =>
    hasTauriRuntime()
      ? invoke<DeploymentCandidate>(
          "create_jenkins_artifact_deployment_candidate",
          { input },
        )
      : devApiFetch<DeploymentCandidate>(
          "/jenkins/artifacts/deployment-candidate",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  createBuildDeploymentDryRun: (
    input: CreateJenkinsBuildDeploymentDryRunInput,
  ) =>
    hasTauriRuntime()
      ? invoke<DeploymentPlan>("create_jenkins_build_deployment_dry_run", {
          input,
        })
      : devApiFetch<DeploymentPlan>("/jenkins/builds/deployment-dry-run", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  listQueue: (connectionKey: string) =>
    hasTauriRuntime()
      ? invoke<JenkinsQueueItem[]>("list_jenkins_queue", { connectionKey })
      : devApiFetch<JenkinsQueueItem[]>(
          `/jenkins/connections/${encodeURIComponent(connectionKey)}/queue`,
        ),
  pollQueueItem: (input: PollJenkinsQueueItemInput) =>
    hasTauriRuntime()
      ? invoke<JenkinsQueueItem>("poll_jenkins_queue_item", { input })
      : devApiFetch<JenkinsQueueItem>("/jenkins/queue/item", {
          method: "POST",
          body: JSON.stringify(input),
        }),
};
