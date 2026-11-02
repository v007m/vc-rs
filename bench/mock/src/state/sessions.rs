use std::collections::HashSet;

/// Issued bearer tokens and the login counter.
#[derive(Default)]
pub struct Sessions {
    logins: u64,
    tokens: HashSet<String>,
}

impl Sessions {
    pub fn login(&mut self) -> String {
        self.logins += 1;
        let token = format!("mock-token-{}", self.logins);
        self.tokens.insert(token.clone());
        token
    }

    pub fn is_valid(&self, token: &str) -> bool {
        self.tokens.contains(token)
    }

    pub const fn logins(&self) -> u64 {
        self.logins
    }
}
