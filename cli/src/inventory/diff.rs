use super::Inventory;

/// Typed resource deltas. Formatting is not involved in deciding what changed.
pub(super) struct ResourceDiff {
    pub removed: Option<Inventory>,
    pub added: Inventory,
    pub localllm_changed: bool,
}

impl ResourceDiff {
    pub fn between(before: Option<&Inventory>, after: &Inventory) -> Self {
        let mut added = after.clone();
        let mut removed = before.cloned();
        let localllm_changed = before.is_none_or(|before| before.localllm != after.localllm);
        if let Some(old) = &mut removed {
            subtract(&mut old.packages, &mut added.packages, PartialEq::eq);
            subtract(&mut old.system, &mut added.system, PartialEq::eq);
            let same_zone = old.time_zone == added.time_zone;
            subtract(&mut old.services, &mut added.services, |a, b| {
                a == b && (same_zone || a.config["StartCalendarInterval"].is_null())
            });
            subtract(&mut old.tools, &mut added.tools, PartialEq::eq);
            subtract(&mut old.skills, &mut added.skills, PartialEq::eq);
        }
        Self {
            removed,
            added,
            localllm_changed,
        }
    }

    pub fn is_empty(&self) -> bool {
        let empty = |inventory: &Inventory| {
            inventory.packages.is_empty()
                && inventory.system.is_empty()
                && inventory.services.is_empty()
                && inventory.tools.is_empty()
                && inventory.skills.is_empty()
        };
        !self.localllm_changed && self.removed.as_ref().is_none_or(empty) && empty(&self.added)
    }
}

fn subtract<T>(before: &mut Vec<T>, after: &mut Vec<T>, equal: impl Fn(&T, &T) -> bool) {
    before.retain(|old| {
        if let Some(index) = after.iter().position(|new| equal(old, new)) {
            after.remove(index);
            false
        } else {
            true
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn inventory() -> Inventory {
        serde_json::from_value(json!({
            "source": "/fixture",
            "packages": [
                {"name": "one", "manager": "Nix", "declared": "1"},
                {"name": "two", "manager": "Nix", "declared": "2"}
            ],
            "system": [{"key": "value", "group": "group", "name": "value", "value": 1}],
            "services": [], "tools": [], "timeZone": "UTC",
            "localllm": {"enabled": false, "default_model": null}
        }))
        .unwrap()
    }

    #[test]
    fn ordering_does_not_create_changes() {
        let before = inventory();
        let mut after = before.clone();
        after.packages.reverse();
        assert!(ResourceDiff::between(Some(&before), &after).is_empty());
    }

    #[test]
    fn values_with_the_same_display_text_are_still_different_values() {
        let before = inventory();
        let mut after = before.clone();
        after.system[0].value = json!("1");
        let changes = ResourceDiff::between(Some(&before), &after);
        assert!(!changes.is_empty());
        assert_eq!(changes.removed.as_ref().unwrap().system[0].value, json!(1));
        assert_eq!(changes.added.system[0].value, json!("1"));
        assert!(changes.added.packages.is_empty());
    }

    #[test]
    fn initial_install_has_additions_without_invented_removals() {
        let changes = ResourceDiff::between(None, &inventory());
        assert!(changes.removed.is_none());
        assert_eq!(changes.added.packages.len(), 2);
        assert!(changes.localllm_changed);
    }

    #[test]
    fn timezone_changes_only_affect_services_with_calendar_schedules() {
        let mut before = inventory();
        before.services = serde_json::from_value(json!([
            {"name": "scheduled", "scope": "user", "config": {"StartCalendarInterval": {"Hour": 1}}},
            {"name": "on-demand", "scope": "user", "config": {}}
        ]))
        .unwrap();
        let mut after = before.clone();
        after.time_zone = "Asia/Tokyo".into();
        let changes = ResourceDiff::between(Some(&before), &after);
        assert_eq!(changes.added.services.len(), 1);
        assert_eq!(changes.added.services[0].name, "scheduled");
    }
}
