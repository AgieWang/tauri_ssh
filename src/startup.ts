/**
 * 保持启动入口足够小，让原生窗口先展示内联启动壳；完整 React 应用在首帧后加载。
 * 不 await 动态导入，否则 WebView 的 load/Finished 会继续被主应用体积阻塞。
 */
function loadApplication() {
  window.requestAnimationFrame(() => {
    void import("./main").catch((error: unknown) => {
      const message = document.querySelector<HTMLElement>(
        ".startup-shell-copy span",
      );
      if (message) {
        message.textContent = "应用加载失败，请重新打开应用";
      }
      console.error("主应用加载失败", error);
    });
  });
}

if (document.readyState === "complete") {
  loadApplication();
} else {
  window.addEventListener("load", loadApplication, { once: true });
}
