import { type BoardRequest, type BoardSnapshot, mountBoardPage } from "./board-start";

export function mountScopedBoard(
  root: HTMLElement | ShadowRoot,
  request: BoardRequest,
  createShell: () => DocumentFragment,
) {
  let container: HTMLElement;
  let unmount: () => void;

  function mount(snapshot?: BoardSnapshot) {
    const next = document.createElement("div");
    next.style.display = "contents";
    next.append(createShell());
    if (container) {
      const install = container.querySelector("#install-app");
      if (install) next.querySelector("#install-app")?.replaceWith(install);
      unmount();
      container.replaceWith(next);
    } else root.append(next);
    container = next;
    unmount = mountBoardPage(container, request, mount, snapshot).unmount;
  }

  mount();
  return () => {
    unmount();
    container.remove();
  };
}
