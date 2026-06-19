use fractal_society::pkgs::skill_graph::{SkillDep, has_cycle, load_order};

fn pos(order: &[String], id: &str) -> usize {
    order
        .iter()
        .position(|candidate| candidate == id)
        .expect("skill id should appear in load order")
}

#[test]
fn acyclic_dependencies_return_valid_topological_order() {
    let deps = vec![
        SkillDep {
            id: "deploy".to_string(),
            depends_on: vec!["test".to_string(), "package".to_string()],
        },
        SkillDep {
            id: "test".to_string(),
            depends_on: vec!["build".to_string()],
        },
        SkillDep {
            id: "package".to_string(),
            depends_on: vec!["build".to_string()],
        },
        SkillDep {
            id: "build".to_string(),
            depends_on: vec![],
        },
    ];

    assert!(!has_cycle(&deps));
    let order = load_order(&deps).expect("acyclic graph should load");

    assert!(pos(&order, "build") < pos(&order, "test"));
    assert!(pos(&order, "build") < pos(&order, "package"));
    assert!(pos(&order, "test") < pos(&order, "deploy"));
    assert!(pos(&order, "package") < pos(&order, "deploy"));
}

#[test]
fn cyclic_dependencies_are_detected_and_rejected() {
    let deps = vec![
        SkillDep {
            id: "a".to_string(),
            depends_on: vec!["b".to_string()],
        },
        SkillDep {
            id: "b".to_string(),
            depends_on: vec!["c".to_string()],
        },
        SkillDep {
            id: "c".to_string(),
            depends_on: vec!["a".to_string()],
        },
    ];

    assert!(has_cycle(&deps));
    assert!(load_order(&deps).is_err());
}

#[test]
fn missing_referenced_dependency_is_loaded_as_leaf() {
    let deps = vec![SkillDep {
        id: "repo-map".to_string(),
        depends_on: vec!["rg".to_string()],
    }];

    let order = load_order(&deps).expect("missing dependency refs are leaf nodes");

    assert_eq!(order, vec!["rg".to_string(), "repo-map".to_string()]);
}
