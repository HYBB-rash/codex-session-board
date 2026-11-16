import hashlib
from pathlib import Path


SOURCES = {
    "/webview/assets/async/157279.bd3d678c15.js":
        "0627b10d53b032a809e94047f2a460cb53446cf8ddc8bdb5e0171dbf7f28ce5d",
    "/webview/assets/async/167392.7db99d1680.js":
        "eb71ee4a68c75517bcf04c527d25224616cace3b042f5b63614492f689af83a3",
    "/webview/assets/async/183533.4dea558bc2.js":
        "b99af3c942effba65799f8da09fdfa2bb49053aa0b2f1cef16c6d98d41599531",
    "/webview/assets/async/283497.67bf9fca70.js":
        "7f2a5f614ab10a75614abc32ec767ea8493452e35eaf20c41b4bf8602c821229",
    "/webview/assets/async/486887.8844429c7e.js":
        "b3105263cb15643ec4abe4aa7f96cf1f43ee56b7aa18c46dfe6ceb4b6a80a240",
    "/webview/assets/async/907538.0572f09d40.js":
        "eba5577e16a50a8106a64a05025fa23d0a589d8b4f775a6fe065ba7d916e6b1c",
    "/webview/assets/async/992177.a875455706.js":
        "4c6e99e53b2317c521574b0b635245efe0732f5c56c5a2523453ef4010dc57dc",
}
MAIN = "/.vite/build/early-bootstrap-226-2f6bd181558008cf.js"
FEDORA_SOURCES = {
    path: digest for path, digest in SOURCES.items()
    if "/486887." not in path and "/907538." not in path
}
FEDORA_SOURCES.update({
    "/webview/assets/async/486887.e71d287279.js":
        "036707749062f71d2912411f6ecfe30dc689341826a909acf455505bbdcc23e0",
    "/webview/assets/async/907538.ce3dfb2b2a.js":
        "7c85ab4378e17c598fdda8128b6a7161f2d79c68d076dc977a2e6a04168fd9a5",
})


def replace(source, anchor, replacement):
    if source.count(anchor) != 1:
        raise ValueError(f"Unsupported target draft boundary: {anchor}")
    return source.replace(anchor, replacement)


def patch_sources(sources):
    selected = FEDORA_SOURCES if any("/486887.e71" in path for path in sources) else SOURCES
    for path, digest in selected.items():
        if path not in sources or hashlib.sha256(sources[path]).hexdigest() != digest:
            raise ValueError(f"Unsupported target draft renderer: {path}")
    changes = {}
    for path in selected:
        source = sources[path].decode()
        if "/157279." in path:
            source = replace(source, ",z.urlPrefillKey=(0,nU.E)(N)",
                ',G?.nativeTargetDraft!=null&&(z.nativeTargetDraft='
                '(0,t5.registerNativeTargetDraft)(G.nativeTargetDraft),'
                'z.homeDraftClientThreadId=z.nativeTargetDraft.clientThreadId),'
                'z.urlPrefillKey=(0,nU.E)(N)')
        elif "/167392." in path:
            source = replace(source, "projectAssignment:q,baseInstructions",
                "projectAssignment:q,nativeTargetDraft,baseInstructions")
            source = replace(source, "workspaceKind:L,projectAssignment:q,mode:",
                "workspaceKind:L,projectAssignment:q,nativeTargetDraft,mode:")
        elif "/183533." in path:
            source = replace(source, "projectAssignment:D})",
                "projectAssignment:D,nativeTargetDraft})")
            source = replace(source, "workspaceKind:U,projectAssignment:D,",
                "workspaceKind:U,projectAssignment:D,nativeTargetDraft,")
        elif "/283497." in path:
            source = replace(source, "nq=(0,i._U)(u.qX,nB)",
                "nq=((target,host)=>target?.hostId??host)"
                "((0,Y.nativeTargetDraft)(ny),(0,i._U)(u.qX,nB))")
            source = replace(source, "let nC=j,nI=",
                "let nativeTarget=(0,Y.nativeTargetDraft)(ny),"
                "nativeWorkspace=(0,i._U)(Y.nativeTargetWorkspace);"
                'if(nativeTarget)e9=(0,o.jsxs)("div",{role:"status",children:['
                'nativeWorkspace?.error??nativeTarget.name??"Loading target project",'
                'nativeTarget.cwd?" · "+nativeTarget.cwd:""]});let nC=j,nI=')
        elif "/486887." in path:
            source = patch_submit(source)
        elif "/907538." in path:
            runtime = (Path(__file__).parent / "target.js").read_text().strip()
            declaration = ('let nativePendingDeletions=(0,f.Kn)'
                '(a.Q2Y.APP_SERVER_PENDING_PROJECT_DELETIONS_BY_HOST,{});'
                'let nativeTargetDrafts=(' + runtime + ')({'
                'createId:()=>(0,a.xa2)(),'
                'scopeId:e=>e.chain?.get(w.id)?.value?.clientThreadId??'
                'e.value?.browserTabMentionConversationId??e.value?.clientThreadId,'
                'existing:(e,t)=>e.get(n("gZd7").H,t),'
                'readContext:(scope,target)=>({'
                'accountId:scope.get(n("SPnB").chatGptAuthenticatedAccountId$),'
                'userId:scope.get(n("SPnB").chatGptAuthenticatedUserId$),'
                'loading:scope.get(n("SPnB").PO).authLoading,'
                'pendingDeletion:Object.entries(scope.get(nativePendingDeletions)??{})'
                '.some(([identity,projects])=>identity.startsWith(target.hostId+":")&&'
                'Object.values(projects).some(project=>project.projectId===target.projectId))}),'
                'request:(e,t,r,i)=>(0,n("Dgwo").V)(e,t).sendRequest(r,i,{priority:"critical"})});'
                'n.d(t,{nativeTargetDraft:()=>nativeTargetDrafts.get,'
                'registerNativeTargetDraft:()=>nativeTargetDrafts.register,'
                'resolveNativeTargetDraft:()=>nativeTargetDrafts.resolve,'
                'nativeTargetCreationInputs:()=>nativeTargetDrafts.creationInputs,'
                'nativeTargetAccepted:()=>nativeTargetDrafts.accepted,'
                'nativeTargetBeforeFirstTurn:()=>nativeTargetDrafts.beforeFirstTurn,'
                'verifyNativeTargetThread:()=>nativeTargetDrafts.verify});'
                'let nativeTargetWorkspace=(0,i.vP)(w,null);'
                'n.d(t,{nativeTargetWorkspace:()=>nativeTargetWorkspace,'
                'initializeNativeTargetDraft:()=>function(scope,target){'
                'let active=true;scope.set(nativeTargetWorkspace,{status:"loading"});'
                'nativeTargetDrafts.resolve(scope,target).then(workspace=>{'
                'if(active)scope.set(nativeTargetWorkspace,{status:"ready",workspace})},error=>{'
                'if(active)scope.set(nativeTargetWorkspace,{status:"error",error:String(error)})});'
                'return()=>{active=false}}});')
            source = replace(source, 'function O(e,t){let n=e.chain.get(w.id)',
                declaration + 'function O(e,t){let n=e.chain.get(w.id)')
            source = normalize(source)
        elif "/992177." in path:
            source = patch_inputs(source)
        changes[path] = source.encode()
    return changes


def normalize(source):
    return replace(source, "projectAssignment:e.projectAssignment,mode:e.mode",
        "projectAssignment:e.projectAssignment,nativeTargetDraft:e.nativeTargetDraft,mode:e.mode")


def patch_submit(source):
    source = replace(source, "lz=null==lG?i4:null",
        'lz=null==lG&&null==n("V3rO").nativeTargetDraft(lL)?i4:null')
    source = replace(source, "let sl=O,ss=",
        'let nativeTarget=n("V3rO").nativeTargetDraft(lL);'
        '(0,f.useEffect)(()=>null==nativeTarget?void 0:'
        'n("V3rO").initializeNativeTargetDraft(lL,nativeTarget),[lL,nativeTarget]);'
        'let sl=O,ss=')
    source = replace(source, "let ec=f?.hostId??eU,",
        "let nativeTarget=(0,H.nativeTargetDraft)(e,n),"
        "nativeWorkspace=null==nativeTarget?null:await (0,H.resolveNativeTargetDraft)(e,nativeTarget),"
        "ec=nativeTarget?.hostId??f?.hostId??eU,")
    source = replace(source, "await eX(ev,ec)",
        "await eX(null==nativeTarget?ev:{...ev,localProjectId:void 0,"
        "remoteProjectId:void 0,cloudThreadPrototype:void 0},ec)")
    source = replace(source, "let ex=(B=ew,F=f,", "let ex=nativeWorkspace?.workspaceRoots??(B=ew,F=f,")
    source = replace(source, "eC=null==ew.localProjectId&&",
        "eC=null==nativeTarget&&null==ew.localProjectId&&")
    source = replace(source, "o=eG.cloudThreadPrototype?",
        "o=null!=nativeTarget?Promise.resolve(nativeWorkspace):eG.cloudThreadPrototype?")
    source = replace(source, "let f=(q=eG.localProjectId,W=p.projectAssignment,",
        "let f=null!=nativeTarget?void 0:(q=eG.localProjectId,W=p.projectAssignment,")
    source = replace(source, "projectAssignment:f}),B=(0,eo.k)",
        "projectAssignment:f,nativeTargetDraft:nativeTarget}),B=(0,eo.k)")
    disabled = "vY" if "submitDisabled:vY,workspaceRootsForLocalExecution:gV" in source else "vJ"
    source = replace(source, f"submitDisabled:{disabled},workspaceRootsForLocalExecution:gV",
        f'submitDisabled:{disabled}||null!=n("V3rO").nativeTargetDraft(lL),workspaceRootsForLocalExecution:gV')
    source = patch_workspace(source)
    return source


def patch_workspace(source):
    source = replace(source, "let v=[],w=(0,r.i1)(d.PR,",
        'var nativeTargets=n("V3rO");let v=[],w=(0,r.i1)(d.PR,')
    source = replace(source, "(e,{get:t,scope:n})=>{let o=(0,d.ah)(n)",
        "(e,{get:t,scope:n})=>{let nativeTarget=nativeTargets.nativeTargetDraft(n);"
        "if(nativeTarget){let snapshot=t(nativeTargets.nativeTargetWorkspace),"
        "cwd=snapshot?.workspace?.cwd??null,roots=snapshot?.workspace?.workspaceRoots??[],"
        'loading=snapshot?.status!=="ready";return{target:{cwd,activeWorkspaceRoot:cwd,'
        "hostId:nativeTarget.hostId,hostConfig:t(c.fm,nativeTarget.hostId),"
        "isActiveWorkspaceRootLoading:loading},selectedRemoteProject:null,"
        "selectedRemoteProjectId:null,executionRemoteHostId:nativeTarget.hostId,"
        "resolvedCwd:cwd,projectLoading:loading,remoteWorkspaceRoots:roots,"
        "projectKey:(0,p.zp)({cwd,hostId:nativeTarget.hostId,isFollowUp:!1,isLoading:loading})}}"
        "let o=(0,d.ah)(n)")
    source = replace(source, "({selectedProject:e,useScopedProject:t},{get:n,scope:o})=>{",
        "({selectedProject:e,useScopedProject:t},{get:n,scope:o})=>{"
        "if(nativeTargets.nativeTargetDraft(o)){let snapshot=n(nativeTargets.nativeTargetWorkspace),"
        "roots=snapshot?.workspace?.workspaceRoots??[];return{"
        "activeLocalProjectCwd:null,activeWorkspaceRootPaths:roots,projectRootPaths:roots,"
        "threadProjectAssignment:void 0,workspaceBrowserRoot:roots[0]??null,"
        'workspaceRootsLoading:snapshot?.status!=="ready"}}')
    return source


def patch_inputs(source):
    source = replace(source, "let P,U,V=r,N,G=",
        "let nativeTargetScope=e,P,U,V=r,N,G=")
    source = replace(source, "class M{params;constructor(e)",
        'var nativeTargets=r("V3rO");class M{params;constructor(e)')
    source = replace(source, "n=e.cloudEnvironment;if(null!=n)",
        "n=e.cloudEnvironment;if(null!=e.nativeTargetDraft)"
        "await nativeTargets.nativeTargetCreationInputs(this.params.scope,"
        "this.params.hostId,e,r);if(null!=n)")
    source = replace(source, "beforeFirstTurn:z,prepareFirstTurn:M",
        "beforeFirstTurn:null==V.nativeTargetDraft?z:async(...r)=>{"
        "await _.nativeTargetBeforeFirstTurn(e,V.nativeTargetDraft);await z?.(...r)},prepareFirstTurn:M")
    source = replace(source, "await a?.(r)},beforeFirstTurn:",
        "null!=V.nativeTargetDraft&&_.nativeTargetAccepted(V.nativeTargetDraft,r);"
        "await a?.(r)},beforeFirstTurn:")
    source = replace(source, "await O?.(e)},dynamicToolFeatureOverrides:Y",
        "await O?.(e);if(null!=V.nativeTargetDraft&&e.status===\"created\"&&"
        "e.firstTurn?.status===\"accepted\")await _.verifyNativeTargetThread("
        "nativeTargetScope,V.nativeTargetDraft,e.conversationId)},dynamicToolFeatureOverrides:Y")
    return source
