export type DeploymentSourceType = "local" | "git" | "image-store";
export type DeploymentRecipe =
  | "image-store"
  | "1panel-app"
  | "dockerfile-service"
  | "docker-compose"
  | "static-openresty"
  | "static-nginx"
  | "node-pm2"
  | "systemd-binary"
  | "custom-script";
export type DockerBuildMode = "remote" | "local_upload";

export interface DeploymentTarget {
  id: number;
  targetKey: string;
  name: string;
  serverAlias: string;
  recipe: DeploymentRecipe | string;
  sourceType: DeploymentSourceType | string;
  projectPath: string;
  gitUrl: string;
  gitRef: string;
  gitCredentialKey: string;
  dockerBuildMode: DockerBuildMode | string;
  workdir: string;
  deployRoot: string;
  domain: string;
  httpsEnabled: boolean;
  port?: number | null;
  healthCheckUrl: string;
  configJson: string;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface UpsertDeploymentTargetInput {
  id?: number | null;
  targetKey: string;
  name: string;
  serverAlias: string;
  recipe: DeploymentRecipe | string;
  sourceType: DeploymentSourceType | string;
  projectPath?: string | null;
  gitUrl?: string | null;
  gitRef?: string | null;
  gitCredentialKey?: string | null;
  dockerBuildMode?: DockerBuildMode | string | null;
  workdir?: string | null;
  deployRoot?: string | null;
  domain?: string | null;
  httpsEnabled?: boolean | null;
  port?: number | null;
  healthCheckUrl?: string | null;
  configJson?: string | null;
  enabled?: boolean | null;
}

export interface DeploymentGroupTarget {
  targetKey: string;
  targetName: string;
  sortOrder: number;
  enabled: boolean;
}

export interface DeploymentGroupTargetInput {
  targetKey: string;
  sortOrder?: number | null;
  enabled?: boolean | null;
}

export interface DeploymentGroup {
  id: number;
  groupKey: string;
  name: string;
  description: string;
  enabled: boolean;
  targets: DeploymentGroupTarget[];
  createdAt: string;
  updatedAt: string;
}

export interface UpsertDeploymentGroupInput {
  id?: number | null;
  groupKey: string;
  name: string;
  description?: string | null;
  enabled?: boolean | null;
  targets: DeploymentGroupTargetInput[];
}

export interface DeploymentTemplate {
  key: DeploymentRecipe | string;
  name: string;
  description: string;
  scenario: string;
  risk: string;
  supportedSources: string[];
  requiredProfiles: string[];
}

export interface DeploymentEnvironmentProfile {
  key: string;
  name: string;
  description: string;
  category: string;
  checks: string[];
}

export interface DeploymentImageStoreEnv {
  key: string;
  label: string;
  defaultValue: string;
  required: boolean;
  secret: boolean;
}

export interface DeploymentImageStoreApp {
  key: string;
  name: string;
  description: string;
  category: string;
  image: string;
  tag: string;
  defaultPort?: number | null;
  containerPort?: number | null;
  volumePath: string;
  env: DeploymentImageStoreEnv[];
  notes: string[];
}

export interface InstallImageStoreAppInput {
  appKey: string;
  targetKey: string;
  name: string;
  serverAlias: string;
  port?: number | null;
  deployRoot?: string | null;
  imageTag?: string | null;
  envJson?: string | null;
  enabled?: boolean | null;
}

export interface DetectDeploymentProjectInput {
  sourceType: DeploymentSourceType | string;
  projectPath?: string | null;
  gitUrl?: string | null;
  gitRef?: string | null;
  gitCredentialKey?: string | null;
}

export interface DeploymentCandidate {
  key: string;
  name: string;
  recipe: DeploymentRecipe | string;
  confidence: number;
  sourceType: DeploymentSourceType | string;
  workdir: string;
  buildCommand: string;
  startCommand: string;
  artifactDir: string;
  dockerfile: string;
  composeFile: string;
  exposedPorts: number[];
  envFiles: string[];
  detectedFrameworks: string[];
  warnings: string[];
  configJson: string;
}

export interface DeploymentDetectionResult {
  sourceType: DeploymentSourceType | string;
  projectRoot: string;
  gitUrl: string;
  gitRef: string;
  commit: string;
  candidates: DeploymentCandidate[];
  warnings: string[];
}

export interface CreateDeploymentDryRunInput {
  targetKey?: string | null;
  groupKey?: string | null;
}

export interface DeploymentPlan {
  planId: string;
  targetKey: string;
  groupKey: string;
  title: string;
  recipe: DeploymentRecipe | string;
  serverAlias: string;
  status: string;
  risk: string;
  approvalRequired: boolean;
  environment: DeploymentEnvironmentProbe;
  stages: DeploymentPlanStage[];
  warnings: string[];
  createdAt: string;
}

export interface DeploymentEnvironmentProbe {
  serverAlias: string;
  os: string;
  arch: string;
  user: string;
  diskAvailableKb?: number | null;
  dockerVersion: string;
  composeVersion: string;
  nginxVersion: string;
  openrestyVersion: string;
  gitVersion: string;
  portAvailable?: boolean | null;
  domainResolved?: boolean | null;
  checks: DeploymentProbeCheck[];
  rawOutput: string;
}

export interface DeploymentProbeCheck {
  key: string;
  label: string;
  status: string;
  message: string;
}

export interface DeploymentPlanStage {
  key: string;
  title: string;
  risk: string;
  approvalRequired: boolean;
  commandPreview: string;
  summary: string;
  status: string;
}

export interface ExecuteDeploymentRunInput {
  targetKey?: string | null;
  groupKey?: string | null;
  planId?: string | null;
  continueRunId?: string | null;
  createdBy?: string | null;
}

export interface CreateDeploymentRollbackDryRunInput {
  targetKey: string;
  runId?: string | null;
}

export interface ExecuteDeploymentRollbackInput {
  targetKey: string;
  runId?: string | null;
  createdBy?: string | null;
}

export interface ListDeploymentRunsInput {
  targetKey?: string | null;
  groupKey?: string | null;
  status?: string | null;
  limit?: number | null;
}

export interface DeploymentRun {
  id: number;
  runId: string;
  targetKey: string;
  groupKey: string;
  status: string;
  versionLabel: string;
  summary: string;
  planJson: string;
  createdBy: string;
  startedAt?: string | null;
  finishedAt?: string | null;
  createdAt: string;
}

export interface DeploymentRunStep {
  id: number;
  runId: string;
  stepKey: string;
  title: string;
  status: string;
  commandPreview: string;
  stdoutPreview: string;
  stderrPreview: string;
  exitCode?: number | null;
  approvalId?: number | null;
  startedAt?: string | null;
  finishedAt?: string | null;
  createdAt: string;
}

export interface DeploymentRunDetail {
  run: DeploymentRun;
  steps: DeploymentRunStep[];
}

export interface DeploymentAiAdviceInput {
  targetKey?: string | null;
  groupKey?: string | null;
  plan?: DeploymentPlan | null;
  prompt?: string | null;
  providerKey?: string | null;
}

export interface DeploymentAiAdviceResult {
  providerKey: string;
  providerName: string;
  model: string;
  answer: string;
  latencyMs: number;
  generatedAt: string;
}
