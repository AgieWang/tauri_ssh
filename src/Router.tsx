import { Suspense, lazy } from "react";
import {
  Navigate,
  createBrowserRouter,
  RouterProvider,
} from "react-router-dom";
import { AppLayout } from "@/components/layout/AppLayout";

const DashboardPage = lazy(() => import("@/pages/dashboard"));
const HomePage = lazy(() => import("@/pages/home"));
const AboutPage = lazy(() => import("@/pages/about"));
const DatabasePage = lazy(() => import("@/pages/database"));
const DeploymentsPage = lazy(() => import("@/pages/deployments"));
const JenkinsPage = lazy(() => import("@/pages/jenkins"));
const SkillsPage = lazy(() => import("@/pages/skills"));
const ResourceMonitorPage = lazy(() => import("@/pages/resource-monitor"));
const SecureCredentialCodeReviewPage = lazy(
  () => import("@/pages/secure-credentials/code-review"),
);
const KnowledgePage = lazy(() => import("@/pages/knowledge"));
const ProjectCatalogPage = lazy(
  () => import("@/pages/knowledge/catalog/ProjectCatalogPage"),
);
const ProjectOverviewPage = lazy(
  () => import("@/pages/knowledge/catalog/ProjectOverviewPage"),
);
const ProjectSetupPage = lazy(
  () => import("@/pages/knowledge/catalog/ProjectSetupPage"),
);
const ProjectVersionsPage = lazy(
  () => import("@/pages/knowledge/catalog/ProjectVersionsPage"),
);
const ProjectAnalysisPage = lazy(
  () => import("@/pages/knowledge/analysis/ProjectAnalysisPage"),
);
const ProjectGraphPage = lazy(
  () => import("@/pages/knowledge/graph/ProjectGraphPage"),
);
const ProjectQaPage = lazy(() => import("@/pages/knowledge/qa/ProjectQaPage"));
const DocumentCreatePage = lazy(
  () => import("@/pages/knowledge/documents/DocumentCreatePage"),
);
const ProjectDocumentsPage = lazy(
  () => import("@/pages/knowledge/documents/ProjectDocumentsPage"),
);
const ProjectSearchPage = lazy(
  () => import("@/pages/knowledge/search/ProjectSearchPage"),
);
const ProjectEmbeddingPage = lazy(
  () => import("@/pages/knowledge/embedding/ProjectEmbeddingPage"),
);

const prototypePages = () => import("@/pages/prototype");
const OnboardingPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.OnboardingPage })),
);
const ServersPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.ServersPage })),
);
const ServerFormPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.ServerFormPage })),
);
const SshImportPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.SshImportPage })),
);
const VaultPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.VaultPage })),
);
const TerminalPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.TerminalPage })),
);
const ApprovalPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.ApprovalPage })),
);
const LogsPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.LogsPage })),
);
const SftpPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.SftpPage })),
);
const EditorPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.EditorPage })),
);
const ProvidersPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.ProvidersPage })),
);
const McpPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.McpPage })),
);
const JumpServerPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.JumpServerPage })),
);
const AuditPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.AuditPage })),
);
const WorkspacePage = lazy(() =>
  prototypePages().then((module) => ({ default: module.WorkspacePage })),
);
const StatesPage = lazy(() =>
  prototypePages().then((module) => ({ default: module.StatesPage })),
);
const CoveragePage = lazy(() =>
  prototypePages().then((module) => ({ default: module.CoveragePage })),
);
const PrototypeSettingsPage = lazy(() =>
  prototypePages().then((module) => ({
    default: module.PrototypeSettingsPage,
  })),
);

const secureCredentialPages = () => import("@/pages/secure-credentials");
const SecureCredentialOverviewPage = lazy(() =>
  secureCredentialPages().then((module) => ({
    default: module.SecureCredentialOverviewPage,
  })),
);
const SecureCredentialVaultPage = lazy(() =>
  secureCredentialPages().then((module) => ({
    default: module.SecureCredentialVaultPage,
  })),
);
const SecureCredentialGitWorkspacesPage = lazy(() =>
  secureCredentialPages().then((module) => ({
    default: module.SecureCredentialGitWorkspacesPage,
  })),
);
const SecureCredentialSessionsPage = lazy(() =>
  secureCredentialPages().then((module) => ({
    default: module.SecureCredentialSessionsPage,
  })),
);
const SecureCredentialMcpPage = lazy(() =>
  secureCredentialPages().then((module) => ({
    default: module.SecureCredentialMcpPage,
  })),
);
const SecureCredentialAuditPage = lazy(() =>
  secureCredentialPages().then((module) => ({
    default: module.SecureCredentialAuditPage,
  })),
);
const SecureCredentialPoliciesPage = lazy(() =>
  secureCredentialPages().then((module) => ({
    default: module.SecureCredentialPoliciesPage,
  })),
);

function RouteLoading() {
  return (
    <div className="startup-shell" role="status" aria-live="polite">
      <div className="startup-shell-mark" aria-hidden="true">
        SSH
      </div>
      <div className="startup-shell-copy">
        <strong>Tauri SSH</strong>
        <span>正在加载工作台…</span>
      </div>
      <span className="startup-shell-spinner" aria-hidden="true" />
    </div>
  );
}

const router = createBrowserRouter([
  {
    path: "/",
    element: <AppLayout />,
    children: [
      { index: true, element: <DashboardPage /> },
      { path: "home", element: <HomePage /> },
      { path: "onboarding", element: <OnboardingPage /> },
      { path: "dashboard", element: <DashboardPage /> },
      { path: "servers", element: <ServersPage /> },
      { path: "server-form", element: <ServerFormPage /> },
      { path: "ssh-import", element: <SshImportPage /> },
      { path: "vault", element: <VaultPage /> },
      {
        path: "secure-credentials/overview",
        element: <SecureCredentialOverviewPage />,
      },
      {
        path: "secure-credentials/vault",
        element: <SecureCredentialVaultPage />,
      },
      {
        path: "secure-credentials/git-workspaces",
        element: <SecureCredentialGitWorkspacesPage />,
      },
      {
        path: "secure-credentials/code-review",
        element: <SecureCredentialCodeReviewPage />,
      },
      {
        path: "secure-credentials/sessions",
        element: <SecureCredentialSessionsPage />,
      },
      { path: "secure-credentials/mcp", element: <SecureCredentialMcpPage /> },
      {
        path: "secure-credentials/audit",
        element: <SecureCredentialAuditPage />,
      },
      {
        path: "secure-credentials/policies",
        element: <SecureCredentialPoliciesPage />,
      },
      { path: "terminal", element: <TerminalPage /> },
      { path: "approval", element: <ApprovalPage /> },
      { path: "logs", element: <LogsPage /> },
      { path: "sftp", element: <SftpPage /> },
      { path: "database", element: <DatabasePage /> },
      { path: "resource-monitor", element: <ResourceMonitorPage /> },
      { path: "deployments", element: <DeploymentsPage /> },
      { path: "jenkins", element: <JenkinsPage /> },
      { path: "editor", element: <EditorPage /> },
      { path: "providers", element: <ProvidersPage /> },
      { path: "mcp", element: <McpPage /> },
      { path: "skills", element: <SkillsPage /> },
      {
        path: "knowledge",
        // 未启用目录阶段时，项目页会指向这里完成线性启用；不能再重定向回被拒绝的目录页。
        element: <KnowledgePage />,
      },
      { path: "knowledge/projects", element: <ProjectCatalogPage /> },
      { path: "knowledge/projects/new", element: <ProjectSetupPage /> },
      {
        path: "knowledge/projects/:projectId/setup",
        element: <ProjectSetupPage />,
      },
      {
        path: "knowledge/projects/:projectId/overview",
        element: <ProjectOverviewPage />,
      },
      {
        path: "knowledge/projects/:projectId/analysis",
        element: <ProjectAnalysisPage />,
      },
      {
        path: "knowledge/projects/:projectId/graph",
        element: <ProjectGraphPage />,
      },
      {
        path: "knowledge/projects/:projectId/qa",
        element: <ProjectQaPage />,
      },
      {
        path: "knowledge/projects/:projectId/versions",
        element: <ProjectVersionsPage />,
      },
      {
        path: "knowledge/projects/:projectId/documents",
        element: <ProjectDocumentsPage />,
      },
      {
        path: "knowledge/projects/:projectId/documents/new",
        element: <DocumentCreatePage />,
      },
      {
        path: "knowledge/projects/:projectId/search",
        element: <ProjectSearchPage />,
      },
      {
        path: "knowledge/projects/:projectId/embedding",
        element: <ProjectEmbeddingPage />,
      },
      { path: "jumpserver", element: <JumpServerPage /> },
      { path: "audit", element: <AuditPage /> },
      { path: "workspace", element: <WorkspacePage /> },
      { path: "states", element: <StatesPage /> },
      { path: "coverage", element: <CoveragePage /> },
      {
        path: "settings",
        element: <Navigate to="/prototype-settings" replace />,
      },
      { path: "prototype-settings", element: <PrototypeSettingsPage /> },
      { path: "about", element: <AboutPage /> },
    ],
  },
]);

export function AppRouter() {
  return (
    <Suspense fallback={<RouteLoading />}>
      <RouterProvider router={router} />
    </Suspense>
  );
}
