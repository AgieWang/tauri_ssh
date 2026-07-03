import { Navigate, createBrowserRouter, RouterProvider } from "react-router-dom";
import { AppLayout } from "@/components/layout/AppLayout";
import HomePage from "@/pages/home";
import AboutPage from "@/pages/about";
import DatabasePage from "@/pages/database";
import DeploymentsPage from "@/pages/deployments";
import SkillsPage from "@/pages/skills";
import ResourceMonitorPage from "@/pages/resource-monitor";
import SecureCredentialCodeReviewPage from "@/pages/secure-credentials/code-review";
import {
  SecureCredentialAuditPage,
  SecureCredentialGitWorkspacesPage,
  SecureCredentialMcpPage,
  SecureCredentialOverviewPage,
  SecureCredentialPoliciesPage,
  SecureCredentialSessionsPage,
  SecureCredentialVaultPage,
} from "@/pages/secure-credentials";
import {
  ApprovalPage,
  AuditPage,
  CoveragePage,
  DashboardPage,
  EditorPage,
  JumpServerPage,
  LogsPage,
  McpPage,
  OnboardingPage,
  ProvidersPage,
  PrototypeSettingsPage,
  ServerFormPage,
  ServersPage,
  SftpPage,
  SshImportPage,
  StatesPage,
  TerminalPage,
  VaultPage,
  WorkspacePage,
} from "@/pages/prototype";

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
      { path: "secure-credentials/overview", element: <SecureCredentialOverviewPage /> },
      { path: "secure-credentials/vault", element: <SecureCredentialVaultPage /> },
      { path: "secure-credentials/git-workspaces", element: <SecureCredentialGitWorkspacesPage /> },
      { path: "secure-credentials/code-review", element: <SecureCredentialCodeReviewPage /> },
      { path: "secure-credentials/sessions", element: <SecureCredentialSessionsPage /> },
      { path: "secure-credentials/mcp", element: <SecureCredentialMcpPage /> },
      { path: "secure-credentials/audit", element: <SecureCredentialAuditPage /> },
      { path: "secure-credentials/policies", element: <SecureCredentialPoliciesPage /> },
      { path: "terminal", element: <TerminalPage /> },
      { path: "approval", element: <ApprovalPage /> },
      { path: "logs", element: <LogsPage /> },
      { path: "sftp", element: <SftpPage /> },
      { path: "database", element: <DatabasePage /> },
      { path: "resource-monitor", element: <ResourceMonitorPage /> },
      { path: "deployments", element: <DeploymentsPage /> },
      { path: "editor", element: <EditorPage /> },
      { path: "providers", element: <ProvidersPage /> },
      { path: "mcp", element: <McpPage /> },
      { path: "skills", element: <SkillsPage /> },
      { path: "jumpserver", element: <JumpServerPage /> },
      { path: "audit", element: <AuditPage /> },
      { path: "workspace", element: <WorkspacePage /> },
      { path: "states", element: <StatesPage /> },
      { path: "coverage", element: <CoveragePage /> },
      { path: "settings", element: <Navigate to="/prototype-settings" replace /> },
      { path: "prototype-settings", element: <PrototypeSettingsPage /> },
      { path: "about", element: <AboutPage /> },
    ],
  },
]);

export function AppRouter() {
  return <RouterProvider router={router} />;
}
