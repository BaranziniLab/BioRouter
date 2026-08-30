use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;
use std::sync::{LazyLock, PoisonError, RwLock};

use serde::{Deserialize, Serialize};

use super::affiliation::InstitutionId;
use super::ExtensionAffiliation;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PrivateAuthority {
    extensions: BTreeMap<String, Option<BTreeSet<String>>>,
}

static AUTHORITY: LazyLock<RwLock<PrivateAuthority>> =
    LazyLock::new(|| RwLock::new(initial_authority()));

fn initial_authority() -> PrivateAuthority {
    #[cfg(test)]
    {
        PrivateAuthority::default()
    }
    #[cfg(not(test))]
    {
        load_from_disk().unwrap_or_default()
    }
}

pub(crate) fn raise_private_extensions(
    entries: impl IntoIterator<Item = (String, String, Option<BTreeSet<String>>)>,
) -> Result<(), String> {
    let mut authority = AUTHORITY.write().unwrap_or_else(PoisonError::into_inner);
    let mut changed = false;
    for (registry_id, extension_name, affiliation) in entries {
        changed |= raise_one(&mut authority.extensions, registry_id, affiliation.clone());
        changed |= raise_one(&mut authority.extensions, extension_name, affiliation);
    }
    if changed {
        persist(&authority)?;
    }
    Ok(())
}

pub(crate) fn resolve(identities: &[String]) -> Option<ExtensionAffiliation> {
    let authority = AUTHORITY.read().unwrap_or_else(PoisonError::into_inner);
    let mut matched = false;
    let mut restriction: Option<BTreeSet<String>> = None;
    for identity in identities {
        let Some(affiliation) = authority.extensions.get(identity) else {
            continue;
        };
        matched = true;
        if let Some(incoming) = affiliation {
            restriction = Some(match restriction {
                Some(current) => current.intersection(incoming).cloned().collect(),
                None => incoming.clone(),
            });
        }
    }
    if !matched {
        return None;
    }
    Some(match restriction {
        None => ExtensionAffiliation::Any,
        Some(ids) => {
            ExtensionAffiliation::institutions(ids.into_iter().map(|id| InstitutionId::new(&id)))
        }
    })
}

#[cfg(test)]
pub(crate) fn insert_test_authority(identity: &str, affiliation: Option<BTreeSet<String>>) {
    let mut authority = AUTHORITY.write().unwrap_or_else(PoisonError::into_inner);
    raise_one(&mut authority.extensions, identity.to_owned(), affiliation);
}

fn raise_one(
    authority: &mut BTreeMap<String, Option<BTreeSet<String>>>,
    identity: String,
    incoming: Option<BTreeSet<String>>,
) -> bool {
    match authority.get_mut(&identity) {
        None => {
            authority.insert(identity, incoming);
            true
        }
        Some(current) => {
            let raised = match (&*current, incoming) {
                (None, Some(incoming)) => Some(incoming),
                (Some(current), Some(incoming)) => {
                    Some(current.intersection(&incoming).cloned().collect())
                }
                (Some(current), None) => Some(current.clone()),
                (None, None) => None,
            };
            if *current == raised {
                false
            } else {
                *current = raised;
                true
            }
        }
    }
}

fn authority_path() -> std::path::PathBuf {
    crate::config::paths::Paths::in_state_dir("marketplace/private-authority.json")
}

#[cfg(not(test))]
fn load_from_disk() -> Result<PrivateAuthority, String> {
    let bytes = match std::fs::read(authority_path()) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(error) => return Err(error.to_string()),
    };
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn persist(authority: &PrivateAuthority) -> Result<(), String> {
    let path = authority_path();
    let parent = path
        .parent()
        .ok_or_else(|| "private authority path has no parent".to_owned())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(authority).map_err(|error| error.to_string())?;
    atomic_write(&path, &bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "private authority path has no parent".to_owned())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.as_file_mut().sync_all())
        .map_err(|error| error.to_string())?;
    file.persist(path)
        .map_err(|error| error.error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_only_raises_and_affiliation_becomes_more_restrictive() {
        let mut authority = BTreeMap::new();
        assert!(raise_one(&mut authority, "fixture".into(), None));
        assert!(raise_one(
            &mut authority,
            "fixture".into(),
            Some(BTreeSet::from(["ucsf".into(), "stanford".into()])),
        ));
        assert!(raise_one(
            &mut authority,
            "fixture".into(),
            Some(BTreeSet::from(["ucsf".into()])),
        ));
        assert!(!raise_one(&mut authority, "fixture".into(), None));
        assert_eq!(
            authority["fixture"],
            Some(BTreeSet::from(["ucsf".to_owned()]))
        );
    }
}
