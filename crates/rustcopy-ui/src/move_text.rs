//! The sentences of the second step of a move (CATALOGO_COMPORTAMENTI_GUI.md L33). The numbers and the
//! decisions come from `robocopy_ingest::moves`; this only says them, so it is a pure, tested function.

use robocopy_ingest::moves::{MoveOutcome, MovePlan};

use crate::format::human_bytes;

/// What the plan says before the person confirms.
pub fn plan_text(plan: &MovePlan) -> String {
    let mut text = if plan.delete.is_empty() {
        "Nessun file ha una copia identica nella destinazione: non c'è niente da cancellare."
            .to_string()
    } else {
        format!(
            "La verifica è riuscita. Si cancellerebbero {} file ({}) dalle cartelle di origine; le cartelle che restano vuote si tolgono.",
            plan.delete.len(),
            human_bytes(plan.delete_bytes)
        )
    };
    if plan.kept() > 0 {
        text.push_str(&format!(
            "\nRestano al loro posto {} file: {} senza copia nella destinazione, {} con una copia diversa, {} che non sono file normali.",
            plan.kept(),
            plan.kept_missing,
            plan.kept_different,
            plan.kept_not_plain
        ));
        for (path, _) in plan.kept_examples.iter().take(3) {
            text.push_str(&format!("\n  · {}", path.display()));
        }
    }
    text
}

/// What was done, as the person is told it.
pub fn outcome_text(outcome: &MoveOutcome, kept: u64) -> String {
    let mut text = format!(
        "Originali cancellati: {} file ({}).",
        outcome.deleted,
        human_bytes(outcome.deleted_bytes)
    );
    if kept > 0 {
        text.push_str(&format!(" Rimasti al loro posto: {kept}."));
    }
    if outcome.skipped_changed > 0 {
        text.push_str(&format!(
            " {} cambiati nel frattempo: lasciati dove sono.",
            outcome.skipped_changed
        ));
    }
    if outcome.failed > 0 {
        text.push_str(&format!(
            " {} non si sono potuti cancellare",
            outcome.failed
        ));
        if let Some(first) = &outcome.first_error {
            text.push_str(&format!(" (per esempio {first})"));
        }
        text.push('.');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use robocopy_ingest::moves::{KeepReason, PlannedDelete};
    use std::path::PathBuf;
    use std::time::SystemTime;

    fn delete(n: u64) -> PlannedDelete {
        PlannedDelete {
            source: PathBuf::from(format!("C:/a/{n}")),
            dest: PathBuf::from(format!("D:/a/{n}")),
            size: 1024,
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn the_plan_says_how_many_go_and_why_the_others_stay() {
        let plan = MovePlan {
            delete: vec![delete(1), delete(2)],
            delete_bytes: 2048,
            kept_missing: 1,
            kept_different: 2,
            kept_examples: vec![(
                PathBuf::from("C:/a/new.txt"),
                KeepReason::MissingInDestination,
            )],
            ..MovePlan::default()
        };
        let text = plan_text(&plan);
        assert!(
            text.contains("Si cancellerebbero 2 file (2,0 KB)"),
            "{text}"
        );
        assert!(
            text.contains(
                "Restano al loro posto 3 file: 1 senza copia nella destinazione, 2 con una copia diversa, 0"
            ),
            "{text}"
        );
        assert!(text.contains("C:/a/new.txt"), "{text}");
    }

    #[test]
    fn an_empty_plan_says_there_is_nothing_to_delete() {
        let text = plan_text(&MovePlan::default());
        assert!(text.contains("niente da cancellare"), "{text}");
    }

    #[test]
    fn the_outcome_mentions_only_what_happened() {
        let clean = MoveOutcome {
            deleted: 3,
            deleted_bytes: 3072,
            ..MoveOutcome::default()
        };
        assert_eq!(
            outcome_text(&clean, 0),
            "Originali cancellati: 3 file (3,0 KB)."
        );
        let messy = MoveOutcome {
            deleted: 1,
            deleted_bytes: 10,
            skipped_changed: 2,
            failed: 1,
            first_error: Some("C:/a/x: Accesso negato".to_string()),
            ..MoveOutcome::default()
        };
        let text = outcome_text(&messy, 4);
        assert!(text.contains("Rimasti al loro posto: 4"), "{text}");
        assert!(text.contains("2 cambiati nel frattempo"), "{text}");
        assert!(
            text.contains("1 non si sono potuti cancellare (per esempio C:/a/x: Accesso negato)"),
            "{text}"
        );
    }
}
