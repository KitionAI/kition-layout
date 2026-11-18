use super::*;

impl LayoutDocument {
    pub(super) fn settings_premises(&self, command: &str, params: &Value, under: &mut Under) -> Result<(), String> {
        if command == "document.preferences" {
            let mut fields = Vec::new();
            for (key, value) in params.as_object().into_iter().flatten() {
                if let Some(members) = value.as_object() {
                    fields.extend(members.keys().map(|member| format!("setup.{key}.{member}")));
                } else {
                    fields.push(format!("setup.{key}"));
                }
            }
            under.premises.push(Premise::on("layout").fields(fields).positions(Vec::<String>::new()));
        }
        let named = match command {
            "condition.options" | "condition.delete" | "condition.apply" => Some("settings.conditions"),
            "variables.define" | "variables.delete" | "variables.insert" => Some("settings.textVariables"),
            "xref.defineFormat" => Some("settings.xrefFormats"),
            "style.compositeFont.set" => Some("settings.compositeFonts"),
            "swatch.ungroupColorGroup" => Some("settings.colorGroups"),
            "list.define" | "list.delete" => Some("setup.lists"),
            "strokeStyle.new" | "strokeStyle.delete" => Some("settings.strokeStyles"),
            "article.add" | "article.remove" | "article.options" | "article.delete" => Some("settings.articles"),
            _ => None,
        };
        if let Some(field) = named
            && let Some(name) = params.get("name").and_then(Value::as_str)
        {
            let name = serde_json::to_string(name).map_err(crate::engine_error)?;
            under.premises.push(Premise::on("layout").fields([format!("{field}[name={name}]")]).positions(Vec::<String>::new()));
        }
        if matches!(command, "xref.insert" | "xref.setFormat") {
            // These handlers check existence; they do not read the format's definition.
            let format =
                params.get("format").and_then(Value::as_str).or_else(|| (command == "xref.insert").then_some("Full Paragraph & Page Number"));
            if let Some(format) = format {
                let format = serde_json::to_string(format).map_err(crate::engine_error)?;
                under
                    .premises
                    .push(Premise::on("layout").fields([format!("settings.xrefFormats[name={format}].name")]).positions(Vec::<String>::new()));
            }
        }
        if command == "swatch.renameColorGroup" {
            let fields = ["name", "to"]
                .into_iter()
                .filter_map(|key| params.get(key).and_then(Value::as_str).map(|name| (key, name)))
                .map(|(key, name)| {
                    let name = serde_json::to_string(if key == "to" { name.trim() } else { name }).map_err(crate::engine_error)?;
                    Ok(format!("settings.colorGroups[name={name}].name"))
                })
                .collect::<Result<Vec<_>, String>>()?;
            under.premises.push(Premise::on("layout").fields(fields).positions(Vec::<String>::new()));
        }
        if command == "ink.options" && params.get("allToProcess").and_then(Value::as_bool).is_some() {
            // The default false value is absent from the saved InkManager register.
            under.premises.push(Premise::on("layout").fields(["~inkAllToProcess"]).positions(Vec::<String>::new()));
        }
        let fields: &[&str] = match command {
            "object.globalLight" => &["setup.globalLight"],
            "object.primaryTextFrame" => &["setup.primaryStory"],
            "edit.transparencyBlendSpace" => &["setup.blendSpace"],
            "changes.track" => &["setup.trackChanges"],
            "spelling.setWords" | "spelling.addWord" => &["settings.userWords"],
            "hyphenation.addException" | "hyphenation.removeException" => &["settings.hyphenationExceptions"],
            "footnote.options" | "footnote.insert" => &["settings.footnoteOptions"],
            "endnote.options" | "endnote.insert" | "endnote.edit" => &["settings.endnoteOptions", "settings.endnoteStory"],
            // The data source removed, or whose field a placeholder takes (its id and kind).
            "data.source.remove" | "data.placeholder.add" => &["settings.dataMerge.sources"],
            _ => &[],
        };
        if !fields.is_empty() {
            under.premises.push(Premise::on("layout").fields(fields.iter().copied()).positions(Vec::<String>::new()));
        }
        Ok(())
    }
}
