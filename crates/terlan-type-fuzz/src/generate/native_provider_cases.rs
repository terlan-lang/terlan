use std::path::PathBuf;

use serde_json::json;

use crate::checked::{CheckedType, GeneratedBody};
use crate::{GeneratedEnvironment, GeneratedFile, GeneratedFixture};

pub(super) fn dual_provider() -> (GeneratedBody, GeneratedFixture) {
    let mut expression = String::from("let buffer0 = Bytes.from_utf8(\"x\");\n");
    for index in 1..=21 {
        let previous = index - 1;
        expression.push_str(&format!(
            "let buffer{index} = Bytes.concat(buffer{previous}, buffer{previous});\n"
        ));
    }
    expression.push_str(concat!(
        "let alpha = inspect_alpha(buffer21);\n",
        "let beta = inspect_beta(buffer21);\n",
        "alpha * 10 + beta",
    ));
    let body = GeneratedBody::expression(CheckedType::Int, json!(402), expression, "402".into())
        .with_prelude(concat!(
            "import alpha.Provider.{inspect as inspect_alpha}.\n",
            "import beta.Provider.{inspect as inspect_beta}.\n",
            "import std.vm.Bytes."
        ));
    (body, fixture())
}

fn fixture() -> GeneratedFixture {
    let providers = [("alpha", "ALPHA", 40), ("beta", "BETA", 2)];
    let mut files = Vec::new();
    let mut environment = Vec::new();
    for (namespace, upper, reply) in providers {
        files.push(GeneratedFile {
            path: PathBuf::from(format!("providers/{namespace}/terlan.toml")),
            contents: provider_manifest(namespace, upper),
            executable: false,
        });
        files.push(GeneratedFile {
            path: PathBuf::from(format!(
                "providers/{namespace}/src/{namespace}/Provider.terl"
            )),
            contents: provider_source(namespace),
            executable: false,
        });
        let helper = PathBuf::from(format!("helpers/{namespace}-helper.sh"));
        files.push(GeneratedFile {
            path: helper.clone(),
            contents: helper_script(reply),
            executable: true,
        });
        environment.push(GeneratedEnvironment {
            name: format!("TERLAN_{upper}_NATIVE_BOUNDARY_HELPER_PATH"),
            executable: helper,
        });
    }
    GeneratedFixture { files, environment }
}

fn provider_manifest(namespace: &str, upper: &str) -> String {
    format!(
        concat!(
            "[package]\n",
            "name = \"type-fuzz-{namespace}-provider\"\n",
            "version = \"0.0.0\"\n",
            "namespace = \"{namespace}\"\n\n",
            "[build]\n",
            "source_roots = [\"src\"]\n",
            "artifact = \"library\"\n\n",
            "[native.rust]\n",
            "crate = \"type_fuzz_{namespace}_native\"\n",
            "path = \"native\"\n",
            "helper = \"type-fuzz-{namespace}-helper\"\n",
            "helper_env = \"TERLAN_{upper}_NATIVE_BOUNDARY_HELPER_PATH\"\n",
        ),
        namespace = namespace,
        upper = upper,
    )
}

fn provider_source(namespace: &str) -> String {
    format!(
        concat!(
            "module {namespace}.Provider.\n\n",
            "import type std.vm.Bytes.{{Bytes}}.\n\n",
            "@compiler.native\n",
            "pub inspect(payload: Bytes): Int.\n",
        ),
        namespace = namespace,
    )
}

fn helper_script(reply: i64) -> String {
    format!(
        concat!(
            "#!/bin/sh\n",
            "request=\n",
            "next=0\n",
            "while IFS=' ' read -r marker id index final data extra; do\n",
            "  test \"$marker\" = chunk || exit 21\n",
            "  test -z \"$extra\" || exit 22\n",
            "  test -n \"$data\" || exit 23\n",
            "  if test -z \"$request\"; then request=$id; fi\n",
            "  test \"$id\" = \"$request\" || exit 24\n",
            "  test \"$index\" = \"$next\" || exit 25\n",
            "  next=$((next + 1))\n",
            "  if test \"$final\" = 1; then\n",
            "    test \"$next\" -ge 2 || exit 26\n",
            "    printf 'reply %s 1 ok_int {reply}\\n' \"$id\"\n",
            "    request=\n",
            "    next=0\n",
            "  else\n",
            "    test \"$final\" = 0 || exit 27\n",
            "  fi\n",
            "done\n",
        ),
        reply = reply,
    )
}
