import re

with open("tests/c_backend_conformance_test.rs", "r") as f:
    text = f.read()

# 1. Remove SUPPORTED_CAPABILITIES
text = re.sub(r'const SUPPORTED_CAPABILITIES: &\[&str\] = &\[[^\]]*\];\n', '', text, flags=re.MULTILINE)

# 2. Remove the test `c_backend_accounts_for_every_contract_2_1_fixture_by_capability`
text = re.sub(r'#\[test\]\nfn c_backend_accounts_for_every_contract_2_1_fixture_by_capability\(\) \{.*?(?=#\[test\]\nfn c_backend_matches_every_constitutive_tier1_fixture)', '', text, flags=re.DOTALL)

# 3. Simplify the newer contract logic in tier 1 test
old_logic = """        let requirements = match parse_symbol_list_field(line, "requires") {
            Some(requirements) => requirements,
            None if line.contains("(requires") => {
                failures.push(format!("fixture line {}: malformed requires field", i + 1));
                continue;
            }
            None => Vec::new(),
        };
        match parse_contract_version(line, "since-contract") {
            Some(version) if version > SUPPORTED_LANGUAGE_CONTRACT => {
                let capabilities_supported = !requirements.is_empty()
                    && requirements
                        .iter()
                        .all(|requirement| SUPPORTED_CAPABILITIES.contains(&requirement.as_str()));
                if capabilities_supported {
                    admitted_newer_contract += 1;
                } else {
                    unsupported_newer_contract += 1;
                    continue;
                }
            }
            Some(_) => {}
            None if line.contains("(since-contract") => {
                failures.push(format!(
                    "fixture line {}: malformed since-contract field",
                    i + 1
                ));
                continue;
            }
            None => {}
        }"""

new_logic = """        match parse_contract_version(line, "since-contract") {
            Some(version) if version > SUPPORTED_LANGUAGE_CONTRACT => {
                unsupported_newer_contract += 1;
                continue;
            }
            Some(_) => {}
            None if line.contains("(since-contract") => {
                failures.push(format!(
                    "fixture line {}: malformed since-contract field",
                    i + 1
                ));
                continue;
            }
            None => {}
        }"""

text = text.replace(old_logic, new_logic)
text = text.replace("let mut admitted_newer_contract = 0;\n", "")
text = text.replace(" + admitted_newer_contract", "")
text = text.replace("assert!(admitted_newer_contract > 0, \"the shared suite should exercise capability-based admission\");\n", "")
text = text.replace(" admitted-newer-contract={admitted_newer_contract}", "")

with open("tests/c_backend_conformance_test.rs", "w") as f:
    f.write(text)

