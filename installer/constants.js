"use strict";

const SENTINEL = "CODEX_ENHANCER_LOADER_V1";
const PATCH_VERSION = "0.1.0";
const LINK_NAME = ".codex-enhancer-link.json";
const APP_ID = "codex-enh-poc";

function loaderStub() {
  return (
    `/*${SENTINEL}*/` +
    `;try{const fs=require("fs"),path=require("path"),linkPath=path.join(process.resourcesPath,"..","${LINK_NAME}");` +
    `if(fs.existsSync(linkPath)){const link=JSON.parse(fs.readFileSync(linkPath,"utf8"));` +
    `const home=path.resolve(String(link.home||""));const runtime=path.resolve(String(link.runtime||""));` +
    `if(home&&runtime.startsWith(home+path.sep)&&fs.existsSync(runtime))require(runtime)}}` +
    `catch(e){console.error("${SENTINEL}",e)}\n`
  );
}

module.exports = {
  SENTINEL,
  PATCH_VERSION,
  LINK_NAME,
  APP_ID,
  loaderStub,
};
