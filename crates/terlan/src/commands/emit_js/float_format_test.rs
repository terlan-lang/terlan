#[test]
fn javascript_float_format_matches_finite_native_values() {
    let values = [
        0.0_f64,
        -0.0,
        1.5,
        -2.25,
        1e-7,
        1e21,
        1e100,
        f64::MIN_POSITIVE,
        f64::MAX,
    ];
    let mut source = format!("const format = {};\n", super::FORMAT);
    for value in values {
        source.push_str(&format!(
            "if (format({value:?}) !== {:?}) throw Error('float mismatch');\n",
            value.to_string()
        ));
    }
    source.push_str("for (const x of [NaN, Infinity, -Infinity]) { let rejected = false; try { format(x); } catch { rejected = true; } if (!rejected) throw Error('non-finite value accepted'); }");
    let output = std::process::Command::new("node")
        .args(["--eval", &source])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
