#[derive(Default, Debug)]
pub struct Hold {
    pub kind: Option<&'static str>,
    pub prior: Option<bool>,
}

impl Hold {
    pub fn press(&mut self, kind: &'static str, current: bool) -> Option<bool> {
        if self.prior.is_some() {
            return None;
        }
        self.kind = Some(kind);
        self.prior = Some(current);
        Some(kind == "ptc")
    }

    pub fn release(&mut self, kind: &str) -> Option<bool> {
        if self.kind != Some(kind) {
            return None;
        }
        self.kind = None;
        self.prior.take()
    }

    pub fn toggled(&mut self) -> Option<bool> {
        let p = self.prior.as_mut()?;
        *p = !*p;
        Some(*p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn talk_restores_prior_state() {
        let mut h = Hold::default();
        assert_eq!(h.press("ptt", true), Some(false));
        assert_eq!(h.release("ptt"), Some(true));
        assert_eq!(h.press("ptt", false), Some(false));
        assert_eq!(h.release("ptt"), Some(false));
    }

    #[test]
    fn cough_mutes_while_held() {
        let mut h = Hold::default();
        assert_eq!(h.press("ptc", false), Some(true));
        assert_eq!(h.release("ptc"), Some(false));
    }

    #[test]
    fn repeat_press_and_other_hold_are_ignored() {
        let mut h = Hold::default();
        h.press("ptt", true);
        assert_eq!(h.press("ptt", false), None);
        assert_eq!(h.press("ptc", false), None);
        assert_eq!(h.release("ptc"), None);
        assert_eq!(h.release("ptt"), Some(true));
    }

    #[test]
    fn toggle_during_hold_changes_what_is_restored() {
        let mut h = Hold::default();
        h.press("ptt", true);
        assert_eq!(h.toggled(), Some(false));
        assert_eq!(h.release("ptt"), Some(false));
        assert_eq!(h.toggled(), None);
    }

    #[test]
    fn release_without_press_does_nothing() {
        assert_eq!(Hold::default().release("ptt"), None);
    }
}
