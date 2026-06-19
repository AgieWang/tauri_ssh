// 移动端 SPA 入口
//
// 与桌面端完全独立：独立 ReactDOM、独立路由、独立 store、独立主题。
// 桌面端入口在 src/main.tsx。

import React from "react";
import ReactDOM from "react-dom/client";
import MobileApp from "./MobileApp";
import "./theme.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <MobileApp />
  </React.StrictMode>,
);
