use super::*;

#[test]
fn solves_transitive_graph_with_backtracking_and_lock_reuse() {
    let packages = fixtures();
    let locked = BTreeMap::from([("math".into(), "1.0.0".into())]);
    let solved = solve_graph(
        vec![requirement("app", "math", ">=1.0.0, <3.0.0")],
        &locked,
        &BTreeSet::new(),
        false,
        |package| Ok(packages.get(package).cloned().unwrap_or_default()),
    )
    .unwrap();
    assert_eq!(solved["math"].version, "1.0.0");
    assert_eq!(solved["core"].version, "1.5.0");

    let solved = solve_graph(
        vec![requirement("app", "math", ">=1.0.0, <3.0.0")],
        &locked,
        &BTreeSet::from(["math".into()]),
        false,
        |package| Ok(packages.get(package).cloned().unwrap_or_default()),
    )
    .unwrap();
    assert_eq!(solved["math"].version, "2.0.0");
    assert_eq!(solved["core"].version, "2.5.0");
}

#[test]
fn optional_constraints_activate_only_for_otherwise_required_packages() {
    let packages: BTreeMap<String, Vec<GraphCandidate>> = BTreeMap::from([
        (
            "feature".into(),
            vec![candidate(
                "feature",
                "1.0.0",
                vec![dependency("helper", "<2.0.0", true)],
            )],
        ),
        (
            "helper".into(),
            vec![
                candidate("helper", "2.0.0", vec![]),
                candidate("helper", "1.0.0", vec![]),
            ],
        ),
    ]);
    let without_helper = solve_graph(
        vec![requirement("app", "feature", "*")],
        &BTreeMap::new(),
        &BTreeSet::new(),
        false,
        |package| Ok(packages.get(package).cloned().unwrap_or_default()),
    )
    .unwrap();
    assert!(!without_helper.contains_key("helper"));

    let with_helper = solve_graph(
        vec![
            requirement("app", "feature", "*"),
            requirement("app", "helper", "*"),
        ],
        &BTreeMap::new(),
        &BTreeSet::new(),
        false,
        |package| Ok(packages.get(package).cloned().unwrap_or_default()),
    )
    .unwrap();
    assert_eq!(with_helper["helper"].version, "1.0.0");
}

#[test]
fn conflicts_are_stable_and_yanked_versions_are_not_newly_selected() {
    let packages = fixtures();
    let roots = vec![
        requirement("left", "core", "<2.0.0"),
        requirement("right", "core", ">=2.0.0"),
    ];
    let first = solve_graph(
        roots.clone(),
        &BTreeMap::new(),
        &BTreeSet::new(),
        false,
        |package| Ok(packages.get(package).cloned().unwrap_or_default()),
    )
    .unwrap_err();
    let second = solve_graph(
        roots,
        &BTreeMap::new(),
        &BTreeSet::new(),
        false,
        |package| Ok(packages.get(package).cloned().unwrap_or_default()),
    )
    .unwrap_err();
    assert_eq!(first, second);
    assert!(first.contains("left requires `<2.0.0`; right requires `>=2.0.0`"));

    let yanked: BTreeMap<String, Vec<GraphCandidate>> = BTreeMap::from([(
        "only".into(),
        vec![GraphCandidate {
            package: "only".into(),
            version: "1.0.0".into(),
            yanked: true,
            dependencies: vec![],
        }],
    )]);
    let error = solve_graph(
        vec![requirement("app", "only", "*")],
        &BTreeMap::new(),
        &BTreeSet::new(),
        false,
        |package| Ok(yanked.get(package).cloned().unwrap_or_default()),
    )
    .unwrap_err();
    assert!(error.contains("1.0.0 (yanked)"));
}

fn fixtures() -> BTreeMap<String, Vec<GraphCandidate>> {
    BTreeMap::from([
        (
            "math".into(),
            vec![
                candidate(
                    "math",
                    "2.0.0",
                    vec![dependency("core", ">=2.0.0, <3.0.0", false)],
                ),
                candidate(
                    "math",
                    "1.0.0",
                    vec![dependency("core", ">=1.0.0, <2.0.0", false)],
                ),
            ],
        ),
        (
            "core".into(),
            vec![
                candidate("core", "2.5.0", vec![]),
                candidate("core", "1.5.0", vec![]),
            ],
        ),
    ])
}

fn candidate(package: &str, version: &str, dependencies: Vec<GraphDependency>) -> GraphCandidate {
    GraphCandidate {
        package: package.into(),
        version: version.into(),
        yanked: false,
        dependencies,
    }
}

fn dependency(package: &str, requirement: &str, optional: bool) -> GraphDependency {
    GraphDependency {
        package: package.into(),
        requirement: requirement.into(),
        optional,
    }
}

fn requirement(requested_by: &str, package: &str, requirement: &str) -> GraphRequirement {
    GraphRequirement {
        package: package.into(),
        requirement: requirement.into(),
        requested_by: requested_by.into(),
    }
}
