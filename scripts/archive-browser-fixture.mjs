export function archiveFixture() {
  const batches = [];
  const controls = {loseReply:false,verify:false};
  const handle = (path,method,input,response) => {
    if (!["/api/archive-batches","/api/archive-batches/check","/api/archive-batches/finish","/api/archive"].includes(path)) return false;
    let body, status = 200;
    if (method === "GET") body = {batches};
    else if (path === "/api/archive-batches") {
      const batch = {id:input.id,createdAt:1760000000,finishedAt:null,
        items:input.items.map(item => ({...item,status:"awaiting",message:null}))};
      batches.unshift(batch);
      body = {batch};
    } else {
      const batch = batches.find(entry => entry.id === (input.batchId || input.id));
      if (path === "/api/archive") {
        const item = batch.items.find(entry => entry.id === input.id);
        item.status = controls.loseReply ? "pending" : "archived";
        if (controls.loseReply) {
          status = 502;
          body = {error:"fixture transport error"};
        } else body = {batch,item};
      } else {
        if (path.endsWith("/finish")) batch.finishedAt = 1760000001;
        if (path.endsWith("/check") && controls.verify) {
          for (const item of batch.items) if (item.status === "pending") item.status = "verified";
        }
        body = {batch};
      }
    }
    response.writeHead(status,{"Content-Type":"application/json"}).end(JSON.stringify(body));
    return true;
  };
  return {handle,controls};
}
