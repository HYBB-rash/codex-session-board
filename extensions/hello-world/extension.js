"use strict";

module.exports = {
  id: "hello-world",
  setup(api) {
    api.contribute({ buttonLabel: "Enhancer" });
    api.log("hello-world-loaded", { id: "hello-world" });
  },
};
