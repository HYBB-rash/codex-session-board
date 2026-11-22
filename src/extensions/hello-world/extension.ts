import type { ExtensionApi } from "../../runtime/api.js";

function setup(api: ExtensionApi): void {
  api.contribute({ buttonLabel: "Enhancer" });
  api.log("hello-world-loaded", { id: "hello-world" });
}

export = {
  id: "hello-world",
  setup,
};
