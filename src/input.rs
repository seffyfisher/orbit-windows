//! Input routing is independent of Win32 so key pairing can be tested on any host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Arm,
    Release,
    Cancel,
    Menu(u32),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Decision {
    pub consume: bool,
    pub action: Option<Action>,
}
pub struct Router {
    trigger: u32,
    held: Option<u32>,
    consumed: [u64; 4],
}
impl Router {
    pub fn new(trigger: u32) -> Self {
        Self {
            trigger,
            held: None,
            consumed: [0; 4],
        }
    }
    pub fn reset(&mut self) {
        self.held = None;
        self.consumed = [0; 4];
    }
    pub fn set_trigger(&mut self, trigger: u32) {
        self.trigger = trigger;
    }
    pub fn event(
        &mut self,
        key: u32,
        down: bool,
        injected: bool,
        paused: bool,
        pending: bool,
        visible: bool,
    ) -> Decision {
        if injected || key >= 256 {
            return Decision::default();
        }
        let bit = 1u64 << (key % 64);
        let bucket = &mut self.consumed[key as usize / 64];
        if !down {
            let consume = *bucket & bit != 0;
            *bucket &= !bit;
            let action = if self.held == Some(key) {
                self.held = None;
                Some(Action::Release)
            } else {
                None
            };
            return Decision { consume, action };
        }
        if *bucket & bit != 0 {
            return Decision {
                consume: true,
                action: None,
            };
        }
        if key == self.trigger {
            let first = self.held.is_none();
            self.held = Some(key);
            let consume = !paused && key != 0xA3;
            if consume {
                *bucket |= bit;
            }
            return Decision {
                consume,
                action: if first && !paused {
                    Some(Action::Arm)
                } else {
                    None
                },
            };
        }
        if visible && matches!(key,0x1B|0x0D|0x09|0x25..=0x28|0x31..=0x39|0x5A|0x20) {
            *bucket |= bit;
            return Decision {
                consume: true,
                action: Some(Action::Menu(key)),
            };
        }
        Decision {
            consume: false,
            action: if (pending || visible) && !matches!(key, 0x10 | 0xA0 | 0xA1) {
                Some(Action::Cancel)
            } else {
                None
            },
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_ctrl_shortcuts_are_balanced_and_pass_through() {
        let mut r = Router::new(0xA3);
        assert_eq!(
            r.event(0xA3, true, false, false, false, false),
            Decision {
                consume: false,
                action: Some(Action::Arm)
            }
        );
        assert_eq!(
            r.event(0x43, true, false, false, true, false),
            Decision {
                consume: false,
                action: Some(Action::Cancel)
            }
        );
        assert_eq!(
            r.event(0x43, false, false, false, false, false),
            Decision::default()
        );
        assert_eq!(
            r.event(0xA3, false, false, false, false, false),
            Decision {
                consume: false,
                action: Some(Action::Release)
            }
        );
    }
    #[test]
    fn dedicated_trigger_release_is_delivered_even_when_suppressed() {
        let mut r = Router::new(0x77);
        assert_eq!(
            r.event(0x77, true, false, false, false, false).action,
            Some(Action::Arm)
        );
        assert_eq!(
            r.event(0x77, true, false, false, true, false),
            Decision {
                consume: true,
                action: None
            }
        );
        assert_eq!(
            r.event(0x77, false, false, false, false, true),
            Decision {
                consume: true,
                action: Some(Action::Release)
            }
        );
    }
    #[test]
    fn consumed_key_up_is_suppressed_after_menu_has_closed() {
        let mut r = Router::new(0xA3);
        assert!(r.event(0x1B, true, false, false, false, true).consume);
        assert_eq!(
            r.event(0x1B, false, false, false, false, false),
            Decision {
                consume: true,
                action: None
            }
        );
        assert_eq!(
            r.event(0x41, true, false, false, false, false),
            Decision::default()
        );
    }
    #[test]
    fn changing_trigger_does_not_lose_the_old_release() {
        let mut r = Router::new(0x77);
        r.event(0x77, true, false, false, false, false);
        r.set_trigger(0xA3);
        assert_eq!(
            r.event(0x77, false, false, false, false, false),
            Decision {
                consume: true,
                action: Some(Action::Release)
            }
        );
        assert_eq!(
            r.event(0xA3, true, false, false, false, false).action,
            Some(Action::Arm)
        );
    }
    #[test]
    fn paused_and_injected_events_do_not_activate_or_consume() {
        let mut r = Router::new(0x77);
        assert_eq!(
            r.event(0x77, true, true, false, false, false),
            Decision::default()
        );
        assert_eq!(
            r.event(0x77, true, false, true, false, false),
            Decision::default()
        );
        assert!(!r.event(0x77, false, false, true, false, false).consume);
    }
    #[test]
    fn all_eight_layout_number_keys_are_keyboard_accessible() {
        let mut r = Router::new(0xA3);
        for key in 0x31..=0x39 {
            assert_eq!(
                r.event(key, true, false, false, false, true).action,
                Some(Action::Menu(key))
            );
            assert!(r.event(key, false, false, false, false, false).consume);
        }
    }
    #[test]
    fn reset_recovers_after_release_on_a_locked_desktop() {
        let mut r = Router::new(0x77);
        r.event(0x77, true, false, false, false, false);
        r.event(0x1B, true, false, false, false, true);
        r.reset();
        assert_eq!(
            r.event(0x77, true, false, false, false, false).action,
            Some(Action::Arm)
        );
        assert!(!r.event(0x1B, false, false, false, false, false).consume);
    }
}
