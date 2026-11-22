export const SENTINEL = "CODEX_ENHANCER_LOADER_V1";
export const PATCH_VERSION = "0.3.0";
export const LINK_NAME = ".codex-enhancer-link.json";
export const APP_ID = "codex-enh-poc";
export const IN_PLACE_APP = "/opt/codex-desktop";
export const IN_PLACE_ROOT = "/var/lib/codex-enhancer";
export const IN_PLACE_ARCHIVE = "resources/app.asar";
export const IN_PLACE_RECEIPT = "installation.json";

export function inPlaceLoader(): string {
    return (
        `/*${SENTINEL}*/` +
        `;try{require("${IN_PLACE_ROOT}/control/dist/installer/in-place-bootstrap.js").startInPlace()}` +
        `catch(e){console.error("${SENTINEL}",e)}\n`
    );
}

export function loaderStub(): string {
    return (
        `/*${SENTINEL}*/` +
        `;try{const fs=require("fs"),path=require("path"),linkPath=path.join(process.resourcesPath,process.platform==="darwin"?"":"..","${LINK_NAME}");` +
        `if(fs.existsSync(linkPath)){const link=JSON.parse(fs.readFileSync(linkPath,"utf8"));` +
        `const home=path.resolve(String(link.home||""));const runtime=path.resolve(String(link.runtime||""));` +
        `if(home){const store=path.join(home,"state","releases"),pointer=path.join(store,"active.json");` +
        `const id=fs.existsSync(pointer)?JSON.parse(fs.readFileSync(pointer,"utf8")).current:null;` +
        `const selected=typeof id==="string"&&/^[a-f0-9]{64}$/.test(id)?path.join(store,id,"dist","runtime","main.js"):runtime;` +
        `if(selected.startsWith(home+path.sep)&&fs.existsSync(selected))require(selected)}}}` +
        `catch(e){console.error("${SENTINEL}",e)}\n`
    );
}
