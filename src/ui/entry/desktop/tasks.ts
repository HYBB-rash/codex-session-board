import { createApp } from "vue";
import type { MountTasks } from "../../../runtime/preload";
import boardStyles from "../../page.css?inline";
import TaskSidebar from "./TaskSidebar.vue";
import taskStyles from "./tasks.css?inline";

export const mountTaskSidebar: MountTasks = (container, board) => {
    const root = container.attachShadow({ mode: "open" });
    const styles = document.createElement("style");
    styles.textContent = `${boardStyles}\n${taskStyles}`;
    const mount = document.createElement("div");
    root.append(styles, mount);
    const app = createApp(TaskSidebar, { board });
    app.mount(mount);
    return () => app.unmount();
};
