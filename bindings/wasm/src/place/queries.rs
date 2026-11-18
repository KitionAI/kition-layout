use super::*;

pub(super) const QUERY_FIELDS: [&str; 6] = ["queryFill", "queryStroke", "queryWeight", "queryKind", "queryLayer", "queryLabel"];

impl LayoutDocument {
    pub(super) fn query_premises(&self, params: &Value, under: &mut Under) -> Result<(), String> {
        let doc = &self.active()?.doc;
        let fields = ["fill", "stroke", "strokeWeight", "opacity"]
            .into_iter()
            .filter(|key| params.get("change").and_then(|v| v.get(*key)).is_some())
            .map(|key| if key == "strokeWeight" { "stroke.weight" } else { key })
            .collect::<Vec<_>>();
        under.objects = kition_layout_engine::cmd::find_objects(doc, params).iter().map(|id| json!(id.0)).collect();
        for id in &under.objects {
            under.premises.push(Premise::on(id.clone()).fields(fields.clone()).positions(Vec::<String>::new()));
        }
        under.covers = Some(Covers { fields: QUERY_FIELDS.into_iter().map(|key| format!("~{key}")).chain(["opacity".into()]).collect() });
        Ok(())
    }

    pub(super) fn query_covered(&self, params: &Value, asks: &[CoveredAsk]) -> Result<CoveredAt, String> {
        let doc = &self.active()?.doc;
        let params = crate::addresses::resolve("find.changeObjects", params, self.session.kdm().map(|kdm| kdm.replica()))?;
        let mut facts = doc
            .all_items()
            .into_iter()
            .filter_map(|id| doc.item(id).map(|item| (id.0, kition_layout_engine::cmd::object_query_facts(item))))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut uncertain = false;
        for ask in asks {
            let Some(id) = numeric_id(&ask.id) else { continue };
            if ask.created {
                facts.remove(&id);
                continue;
            }
            if !ask.unknown.is_empty() {
                uncertain = true;
            }
            if ask.deleted && !QUERY_FIELDS.iter().all(|key| ask.before.contains_key(&format!("~{key}"))) {
                uncertain = true;
            }
            if !ask.deleted && !facts.contains_key(&id) {
                continue;
            }
            let item = facts.entry(id).or_insert_with(|| json!({}));
            for key in QUERY_FIELDS {
                if let Some(before) = ask.before.get(&format!("~{key}")) {
                    item[key] = before.clone();
                }
            }
            if let Some(before) = ask.before.get("opacity") {
                item["queryOpacity"] = before.clone();
            }
        }
        let under = if uncertain {
            vec![]
        } else {
            facts.iter().filter(|(_, facts)| kition_layout_engine::cmd::object_query_matches(doc, &params, facts)).map(|(id, _)| json!(id)).collect()
        };
        let covered = asks
            .iter()
            .map(|ask| Covered {
                id: ask.id.clone(),
                covered: if uncertain { None } else { Some(under.iter().any(|id| numeric_id(id) == numeric_id(&ask.id))) },
                ..Default::default()
            })
            .collect();
        Ok(CoveredAt { under, covered })
    }
}
