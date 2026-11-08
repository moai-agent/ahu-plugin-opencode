//! Version-specific headless controls. Interactive adapters never use these.
//! Claude's disableAllHooks preserves managed hooks, skills and direct MCP:
//! https://code.claude.com/docs/en/hooks#disable-or-remove-hooks
//! Verified on 2.1.283, 2.1.288 and 2.1.289 with synthetic
//! SessionStart/UserPromptSubmit/Stop markers. Homes and authentication stay native.
//! 2.1.302: installed CLI help and vendor hook controls reviewed; synthetic
//! admission/argv tests only, without authenticated headless verification.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    pub id: &'static str,
    pub args: &'static [&'static str],
}

// Never disable hooks or bypass trust. Native strict configuration refuses
// conflicts with mandatory feature requirements. Admission additionally
// requires an explicit null requirements response.

pub fn profile(_harness: &str, version: &str) -> Option<Profile> {
    // Admission and invocation must interpret version decoration identically:
    // a catalog-accepted `v` prefix must not skip mandatory native controls.
    let _version = crate::compatibility::version_token(version)?;
    None
}
