use super::*;

impl App {
    pub(crate) fn is_native_probe(&self, identifier: &str) -> Result<bool> {
        self.mapping_store.is_native_probe(&self.config.host_id, identifier)
    }

    pub(crate) fn retain_business_threads(&self, threads: &mut Vec<Value>) -> Result<()> {
        let owned = self.mapping_store.native_probes(&self.config.host_id)?;
        threads.retain(|thread| !owned.contains(thread["id"].as_str().unwrap_or("")));
        Ok(())
    }

    pub(super) fn retain_business_rows(&self, rows: &mut Vec<Value>) -> Result<()> {
        let mut owned = HashMap::new();
        for row in rows.iter() {
            let host = row["hostId"].as_str().unwrap_or(&self.config.host_id);
            if !owned.contains_key(host) {
                owned.insert(host.to_owned(), self.mapping_store.native_probes(host)?);
            }
        }
        rows.retain(|row| {
            let host = row["hostId"].as_str().unwrap_or(&self.config.host_id);
            let identifier = row["threadId"].as_str().or_else(|| row["id"].as_str()).unwrap_or("");
            !owned[host].contains(identifier)
        });
        Ok(())
    }
}
