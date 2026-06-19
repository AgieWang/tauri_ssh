// 移动端根组件 — HashRouter 是 Tauri Mobile 与 axum 子路径托管的最稳路由方案

import { HashRouter, Routes, Route, Navigate } from "react-router-dom";
import Home from "./pages/Home";

function MobileApp() {
  return (
    <HashRouter>
      <Routes>
        <Route path="/" element={<Home />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </HashRouter>
  );
}

export default MobileApp;
