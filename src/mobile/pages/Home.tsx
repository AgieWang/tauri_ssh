// 移动端默认欢迎页（占位）
//
// 这是骨架，业务页面请在 src/mobile/pages/ 下新增并在 MobileApp.tsx 注册路由。
// 整体架构详见 .claude/skills/mobile-app-architecture/skill.md。

function Home() {
  return (
    <div className="mobile-home">
      <h1>移动端伴侣</h1>
      <p>骨架已就绪。</p>
      <p className="mobile-home-tip">
        💡 开发界面前建议先用{" "}
        <a
          href="https://ai-workstation.ruoyi.plus/"
          target="_blank"
          rel="noopener noreferrer"
        >
          AI 工作站
        </a>{" "}
        设计原型图，再映射到 <code>src/mobile/pages/</code>。
      </p>
      <ul>
        <li>桌面端起 axum 远程网关：见 <code>src-tauri/src/remote/</code></li>
        <li>移动端走 fetch / WebSocket：见 <code>src/mobile/lib/api.ts</code> + <code>ws.ts</code></li>
        <li>新增页面：在 <code>src/mobile/pages/</code> 下创建组件并到 <code>MobileApp.tsx</code> 注册路由</li>
      </ul>
    </div>
  );
}

export default Home;
