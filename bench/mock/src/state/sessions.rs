use std::collections::HashSet;

/// Issued bearer tokens, OAuth refresh tokens and codes, and the counters that name each kind.
#[derive(Default)]
pub struct Sessions {
    logins: u64,
    tokens: HashSet<String>,
    refresh: HashSet<String>,
    codes: HashSet<String>,
    refreshed: u64,
    coded: u64,
}

impl Sessions {
    pub fn login(&mut self) -> String {
        self.logins += 1;
        let token = format!("mock-token-{}", self.logins);
        self.tokens.insert(token.clone());
        token
    }

    /// Issues a refresh token that [`Self::refreshes`] accepts until it is used.
    pub fn refresh_token(&mut self) -> String {
        self.refreshed += 1;
        let token = format!("mock-refresh-{}", self.refreshed);
        self.refresh.insert(token.clone());
        token
    }

    /// Whether `token` was issued and not used yet; using it rotates it out.
    pub fn refreshes(&mut self, token: &str) -> bool {
        self.refresh.remove(token)
    }

    /// Issues a one-time authorization code.
    pub fn code(&mut self) -> String {
        self.coded += 1;
        let code = format!("mock-code-{}", self.coded);
        self.codes.insert(code.clone());
        code
    }

    /// Whether `code` was issued and not exchanged yet; exchanging it uses it up.
    pub fn redeems(&mut self, code: &str) -> bool {
        self.codes.remove(code)
    }

    pub fn is_valid(&self, token: &str) -> bool {
        self.tokens.contains(token)
    }

    pub const fn logins(&self) -> u64 {
        self.logins
    }
}
