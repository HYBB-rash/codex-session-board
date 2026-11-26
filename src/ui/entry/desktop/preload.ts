import { start } from "../../../runtime/preload.js";
import { mountEnhancerBoard } from "./mount";
import { mountTaskSidebar } from "./tasks";

start(mountEnhancerBoard, mountTaskSidebar);
