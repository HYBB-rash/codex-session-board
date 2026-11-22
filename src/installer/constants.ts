export const SENTINEL = "CODEX_ENHANCER_LOADER_V1";
export const PATCH_VERSION = "0.2.0";
export const LINK_NAME = ".codex-enhancer-link.json";
export const APP_ID = "codex-enh-poc";

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
