use super::super::{WriteAction, WriteOptions, WriteResult};
use crate::core::editor_registry::types::EditorTarget;

// ---------------------------------------------------------------------------
// Mistral Vibe TOML writer
//
// Vibe stores MCP servers in ~/.vibe/config.toml as TOML array of tables:
// [[mcp_servers]]
// name = "lean-ctx"
// transport = "stdio"
// command = "lean-ctx"
// args = ["serve"]
// ---------------------------------------------------------------------------

pub(crate) fn write_vibe_toml(
    target: &EditorTarget,
    binary: &str,
    _opts: WriteOptions,
) -> Result<WriteResult, String> {
    // Create the lean-ctx server table
    let mut lean_ctx_server = toml_edit::Table::new();
    lean_ctx_server.insert("name", toml_edit::value("lean-ctx"));
    lean_ctx_server.insert("transport", toml_edit::value("stdio"));
    lean_ctx_server.insert("command", toml_edit::value(binary));

    // Create args array
    let mut args_array = toml_edit::Array::new();
    args_array.push(toml_edit::Value::String(toml_edit::Formatted::new(
        "serve".to_string(),
    )));
    lean_ctx_server.insert(
        "args",
        toml_edit::Item::Value(toml_edit::Value::Array(args_array)),
    );

    if target.config_path.exists() {
        let content = std::fs::read_to_string(&target.config_path).map_err(|e| e.to_string())?;
        let mut doc = content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| e.to_string())?;

        // Check if lean-ctx server already exists
        let already_exists = if let Some(toml_edit::Item::ArrayOfTables(aot)) =
            doc.get_mut("mcp_servers")
        {
            let mut found = false;
            for table in aot.iter_mut() {
                if let Some(toml_edit::Item::Value(toml_edit::Value::String(name))) =
                    table.get("name")
                    && name.value() == "lean-ctx"
                {
                    found = true;
                    // Check if it matches what we want to write
                    let existing_command = table.get("command").and_then(|v| v.as_str());
                    if existing_command == Some(binary) {
                        // Check args exist and match
                        if let Some(toml_edit::Item::Value(toml_edit::Value::Array(existing_args))) =
                            table.get("args")
                            && existing_args.len() == 1
                            && existing_args.get(0).and_then(|v| v.as_str()) == Some("serve")
                        {
                            return Ok(WriteResult {
                                action: WriteAction::Already,
                                note: None,
                            });
                        }
                    }
                    // Update existing entry - replace the table's contents
                    table.clear();
                    table.extend(lean_ctx_server.clone());
                    break;
                }
            }
            if !found {
                aot.push(lean_ctx_server);
            }
            true
        } else {
            // Create new array of tables
            let mut aot = toml_edit::ArrayOfTables::new();
            aot.push(lean_ctx_server.clone());
            doc.insert("mcp_servers", toml_edit::Item::ArrayOfTables(aot));
            false
        };

        if already_exists {
            return Ok(WriteResult {
                action: WriteAction::Already,
                note: None,
            });
        }

        let formatted = doc.to_string();
        crate::config_io::write_atomic_with_backup(&target.config_path, &formatted)?;
        return Ok(WriteResult {
            action: WriteAction::Updated,
            note: None,
        });
    }

    // Create new config file
    let mut doc = toml_edit::DocumentMut::new();
    let mut aot = toml_edit::ArrayOfTables::new();
    aot.push(lean_ctx_server);
    doc.insert("mcp_servers", toml_edit::Item::ArrayOfTables(aot));

    let formatted = doc.to_string();
    crate::config_io::write_atomic_with_backup(&target.config_path, &formatted)?;
    Ok(WriteResult {
        action: WriteAction::Created,
        note: None,
    })
}
