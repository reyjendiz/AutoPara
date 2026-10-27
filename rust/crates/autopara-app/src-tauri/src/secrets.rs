//! Chat provider API keys live in the OS keychain (Windows Credential Manager, macOS Keychain),
//! never in the database or the log, and are never sent back to the interface (plan R9, 7.6).

const SERVICE: &str = "AutoPara Lecture Assist";

fn entry(provider: &str) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, provider)
}

/// Save, or with an empty key forget, the key for one provider preset.
pub fn set_api_key(provider: &str, key: &str) -> Result<(), String> {
    let entry = entry(provider).map_err(|e| e.to_string())?;
    let result = if key.trim().is_empty() {
        entry.delete_credential()
    } else {
        entry.set_password(key.trim())
    };
    match result {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// For the engine only. The interface learns just whether one is saved.
pub fn api_key(provider: &str) -> Option<String> {
    entry(provider).ok()?.get_password().ok()
}

pub fn has_api_key(provider: &str) -> bool {
    api_key(provider).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_saved_replaced_and_forgotten() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        // The mock store keeps nothing between entries, so check within one round trip.
        let entry = entry("deepseek").unwrap();
        entry.set_password("sk-1").unwrap();
        assert_eq!(entry.get_password().unwrap(), "sk-1");
        entry.delete_credential().unwrap();
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
        // Forgetting a key that was never saved is not an error.
        assert!(set_api_key("never-saved", "").is_ok());
    }
}
