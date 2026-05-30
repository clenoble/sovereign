//! Entity disambiguation stage of the PII pipeline.
//!
//! Takes findings from regex + NER and tries to attach each one to an
//! existing `Entity` based on graph context the user already has.
//! Strategies (one per finding kind, applied in order):
//!
//!   - Email → extract the domain, match against any `Entity.domains[]`.
//!   - Phone → match the digits against any `Contact.addresses` where
//!             `channel == Phone`. The contact's `entity_id` then attaches.
//!   - Person / Org name → fuzzy-match against `Entity.name`
//!             (normalized + token-prefix). Conservative: only links on
//!             a clear match to avoid wrong attribution.
//!
//! For unmatched findings of "namable" kinds (email-with-unknown-domain,
//! person_name, org_name), a new `Entity` is proposed and queued for the
//! user to confirm in the dashboard. Other kinds (IBAN, AVS, passport,
//! DOB, IP, address found by regex) typically belong to `Self` and are
//! returned with `entity_id = None`; the ingest hook attributes them to
//! the user's `Self` entity at commit time.
//!
//! Pure data-in / data-out — the entity and contact lists are passed in
//! by the pipeline orchestrator, which fetches them from the DB once
//! per scan. That keeps this module synchronous and easy to test.

use std::collections::HashSet;

use sovereign_db::schema::{ChannelType, Contact, Entity, EntityKind};

use super::{Finding, PiiKind};

/// One finding paired with the entity it was linked to (if any).
#[derive(Debug, Clone)]
pub struct LinkedFinding {
    pub finding: Finding,
    /// Raw entity ID string (e.g. `"entity:abc"`) or `None` if the finding
    /// could not be attached to an existing entity.
    pub entity_id: Option<String>,
}

/// Output of [`disambiguate`].
#[derive(Debug, Clone, Default)]
pub struct DisambiguationResult {
    pub linked: Vec<LinkedFinding>,
    /// New entities proposed for findings that didn't match anything. The
    /// orchestrator forwards these to the dashboard's review queue rather
    /// than committing them silently.
    pub proposed_entities: Vec<Entity>,
}

/// Attach each finding to an entity where possible; propose new entities
/// for namable findings that had no match.
pub fn disambiguate(
    findings: Vec<Finding>,
    entities: &[Entity],
    contacts: &[Contact],
) -> DisambiguationResult {
    // Track domains we've already proposed so two emails on the same
    // unknown domain only generate one new Entity proposal.
    let mut proposed_domains: HashSet<String> = HashSet::new();
    // Track names we've already proposed so duplicate person/org findings
    // don't multiply.
    let mut proposed_names: HashSet<String> = HashSet::new();
    let mut proposed: Vec<Entity> = Vec::new();
    let mut linked: Vec<LinkedFinding> = Vec::with_capacity(findings.len());

    for finding in findings {
        let entity_id = match finding.kind {
            PiiKind::Email => {
                let domain = extract_email_domain(&finding.sample);
                let by_domain = domain.and_then(|d| match_entity_by_domain(d, entities));
                if by_domain.is_none() {
                    if let Some(d) = domain {
                        if proposed_domains.insert(d.to_ascii_lowercase()) {
                            proposed.push(propose_service_entity(d));
                        }
                    }
                }
                by_domain
            }
            PiiKind::Phone => match_entity_by_phone(&finding.sample, contacts),
            PiiKind::PersonName => {
                let by_name = match_entity_by_name(&finding.sample, entities);
                if by_name.is_none() {
                    let key = normalize_name(&finding.sample);
                    if !key.is_empty() && proposed_names.insert(key) {
                        proposed.push(propose_named_entity(
                            finding.sample.clone(),
                            EntityKind::Person,
                        ));
                    }
                }
                by_name
            }
            PiiKind::OrgName => {
                let by_name = match_entity_by_name(&finding.sample, entities);
                if by_name.is_none() {
                    let key = normalize_name(&finding.sample);
                    if !key.is_empty() && proposed_names.insert(key) {
                        proposed.push(propose_named_entity(
                            finding.sample.clone(),
                            EntityKind::Org,
                        ));
                    }
                }
                by_name
            }
            // Self-PII kinds: no entity attribution at this stage.
            // The ingest hook will attribute these to the user's Self
            // entity at commit time.
            _ => None,
        };
        linked.push(LinkedFinding { finding, entity_id });
    }

    DisambiguationResult {
        linked,
        proposed_entities: proposed,
    }
}

/// Extract the domain portion of an email address.
pub fn extract_email_domain(email: &str) -> Option<&str> {
    let at = email.rfind('@')?;
    let domain = &email[at + 1..];
    if domain.is_empty() || !domain.contains('.') {
        None
    } else {
        Some(domain)
    }
}

fn match_entity_by_domain(domain: &str, entities: &[Entity]) -> Option<String> {
    let needle = domain.to_ascii_lowercase();
    for e in entities {
        if e.domains.iter().any(|d| d.eq_ignore_ascii_case(&needle)) {
            return e.id_string();
        }
    }
    None
}

fn match_entity_by_phone(phone: &str, contacts: &[Contact]) -> Option<String> {
    let needle = digits_only(phone);
    if needle.is_empty() {
        return None;
    }
    for c in contacts {
        for addr in &c.addresses {
            if !matches!(addr.channel, ChannelType::Phone) {
                continue;
            }
            if digits_only(&addr.address) == needle {
                return c.entity_id.clone();
            }
        }
    }
    None
}

fn match_entity_by_name(name: &str, entities: &[Entity]) -> Option<String> {
    for e in entities {
        if name_similar(name, &e.name) {
            return e.id_string();
        }
    }
    None
}

/// Lowercase + strip non-alphanumeric (other than whitespace) + collapse
/// runs of whitespace. Stable across punctuation variants like "Acme, AG"
/// vs "Acme AG".
fn normalize_name(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Conservative name-match predicate: exact (after normalization) OR one
/// is a token-prefix of the other ("Acme" ↔ "Acme AG"). Does not handle
/// reordered tokens ("Smith, Alice" vs "Alice Smith") — keeps disambig
/// from over-eagerly linking.
pub fn name_similar(a: &str, b: &str) -> bool {
    let na = normalize_name(a);
    let nb = normalize_name(b);
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    if na == nb {
        return true;
    }
    let (short, long) = if na.len() <= nb.len() {
        (&na, &nb)
    } else {
        (&nb, &na)
    };
    // Token-prefix: long starts with short followed by a space, ensuring
    // we match on whole tokens (so "Al" doesn't match "Alice").
    long.starts_with(short.as_str()) && long.as_bytes().get(short.len()) == Some(&b' ')
}

fn digits_only(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn propose_service_entity(domain: &str) -> Entity {
    let mut e = Entity::new(domain.to_string(), EntityKind::Service);
    e.domains = vec![domain.to_ascii_lowercase()];
    // Proposed entities are not yet user-confirmed: the dashboard's
    // review queue uses is_owned to distinguish "the user accepted this
    // entity into their graph" from "the pipeline inferred this exists".
    e.is_owned = false;
    e
}

fn propose_named_entity(name: String, kind: EntityKind) -> Entity {
    let mut e = Entity::new(name, kind);
    e.is_owned = false;
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(kind: PiiKind, sample: &str, start: usize, confidence: f32) -> Finding {
        Finding {
            kind,
            start,
            end: start + sample.len(),
            sample: sample.into(),
            confidence,
        }
    }

    fn entity_with_domains(name: &str, kind: EntityKind, domains: &[&str]) -> Entity {
        let mut e = Entity::new(name.into(), kind);
        e.domains = domains.iter().map(|d| d.to_string()).collect();
        e
    }

    fn contact_with_phone(name: &str, phone: &str, entity_id: Option<&str>) -> Contact {
        use sovereign_db::schema::ChannelAddress;
        let mut c = Contact::new(name.into(), false);
        c.addresses.push(ChannelAddress {
            channel: ChannelType::Phone,
            address: phone.into(),
            display_name: None,
            is_primary: true,
        });
        c.entity_id = entity_id.map(|s| s.to_string());
        c
    }

    // --- domain extraction ---

    #[test]
    fn extract_domain_basic() {
        assert_eq!(extract_email_domain("alice@example.com"), Some("example.com"));
        assert_eq!(
            extract_email_domain("bob+tag@sub.example.co.uk"),
            Some("sub.example.co.uk")
        );
    }

    #[test]
    fn extract_domain_handles_no_at_or_no_dot() {
        assert_eq!(extract_email_domain("notanemail"), None);
        assert_eq!(extract_email_domain("alice@localhost"), None);
        assert_eq!(extract_email_domain("alice@"), None);
    }

    // --- name normalization & similarity ---

    #[test]
    fn normalize_strips_punctuation_and_collapses_space() {
        assert_eq!(normalize_name("Acme, AG"), "acme ag");
        assert_eq!(normalize_name("  Acme   AG  "), "acme ag");
    }

    #[test]
    fn name_similar_exact_match_after_normalize() {
        assert!(name_similar("Acme AG", "ACME ag"));
        assert!(name_similar("Acme, AG.", "Acme AG"));
    }

    #[test]
    fn name_similar_token_prefix() {
        // Short is a whole-token prefix of long.
        assert!(name_similar("Acme", "Acme AG"));
        assert!(name_similar("Acme AG", "Acme"));
    }

    #[test]
    fn name_similar_rejects_partial_token() {
        // "Al" is a substring but not a whole-token prefix of "Alice".
        assert!(!name_similar("Al", "Alice"));
        // Disjoint surnames.
        assert!(!name_similar("Alice Smith", "Bob Smith"));
    }

    #[test]
    fn name_similar_rejects_reordered_tokens() {
        // Conservative: "Smith Alice" should NOT auto-link to "Alice Smith"
        // — too easy to attach the wrong person.
        assert!(!name_similar("Alice Smith", "Smith Alice"));
    }

    #[test]
    fn name_similar_rejects_empty() {
        assert!(!name_similar("", "anything"));
        assert!(!name_similar("anything", ""));
    }

    // --- email disambiguation ---

    #[test]
    fn email_links_to_existing_entity_by_domain() {
        let mut acme = entity_with_domains("Acme Corp", EntityKind::Org, &["acme.com"]);
        acme.id = Some(sovereign_db::schema::raw_to_thing("entity:acme").unwrap());
        let entities = vec![acme];
        let f = vec![finding(PiiKind::Email, "alice@acme.com", 0, 1.0)];
        let r = disambiguate(f, &entities, &[]);
        assert_eq!(r.linked.len(), 1);
        assert_eq!(r.linked[0].entity_id.as_deref(), Some("entity:acme"));
        assert!(r.proposed_entities.is_empty());
    }

    #[test]
    fn email_unmatched_domain_proposes_service_entity() {
        let f = vec![finding(PiiKind::Email, "alice@unknown.com", 0, 1.0)];
        let r = disambiguate(f, &[], &[]);
        assert_eq!(r.linked.len(), 1);
        assert!(r.linked[0].entity_id.is_none());
        assert_eq!(r.proposed_entities.len(), 1);
        let e = &r.proposed_entities[0];
        assert_eq!(e.kind, EntityKind::Service);
        assert_eq!(e.domains, vec!["unknown.com"]);
        // Proposed, not yet user-confirmed.
        assert!(!e.is_owned);
    }

    #[test]
    fn email_two_findings_same_unknown_domain_propose_once() {
        let f = vec![
            finding(PiiKind::Email, "alice@new.com", 0, 1.0),
            finding(PiiKind::Email, "bob@NEW.com", 30, 1.0),
        ];
        let r = disambiguate(f, &[], &[]);
        assert_eq!(r.proposed_entities.len(), 1);
    }

    // --- phone disambiguation ---

    #[test]
    fn phone_links_when_digits_match_across_formatting() {
        // +41 79 555 12 34 and +41-79-555-12-34 both digit-normalize to
        // 41795551234 and should resolve to Bob's entity.
        let contact = contact_with_phone("Bob", "+41 79 555 12 34", Some("entity:bob"));
        let f = vec![finding(PiiKind::Phone, "+41-79-555-12-34", 0, 1.0)];
        let r = disambiguate(f, &[], &[contact]);
        assert_eq!(r.linked[0].entity_id.as_deref(), Some("entity:bob"));
    }

    #[test]
    fn phone_strict_digits_only_no_country_code_normalization() {
        // String-equality on digit-only forms: +41… vs 041… differ. We
        // intentionally don't try to normalize country-code conventions
        // here — wrong attribution is worse than no attribution, and the
        // dashboard's review queue will catch unmatched phones anyway.
        let contact = contact_with_phone("Bob", "+41 79 555 12 34", Some("entity:bob"));
        let f = vec![finding(PiiKind::Phone, "041 79 555 12 34", 0, 1.0)];
        let r = disambiguate(f, &[], &[contact]);
        assert!(r.linked[0].entity_id.is_none());
    }

    #[test]
    fn phone_unmatched_does_not_propose() {
        let f = vec![finding(PiiKind::Phone, "555-123-4567", 0, 1.0)];
        let r = disambiguate(f, &[], &[]);
        assert!(r.linked[0].entity_id.is_none());
        assert!(r.proposed_entities.is_empty());
    }

    // --- name disambiguation ---

    #[test]
    fn person_name_links_to_existing_entity() {
        let mut alice = Entity::new("Alice Smith".into(), EntityKind::Person);
        alice.id = Some(sovereign_db::schema::raw_to_thing("entity:alice").unwrap());
        let entities = vec![alice];
        let f = vec![finding(PiiKind::PersonName, "Alice Smith", 0, 0.9)];
        let r = disambiguate(f, &entities, &[]);
        assert_eq!(r.linked[0].entity_id.as_deref(), Some("entity:alice"));
        assert!(r.proposed_entities.is_empty());
    }

    #[test]
    fn org_name_token_prefix_match() {
        let mut acme = Entity::new("Acme AG".into(), EntityKind::Org);
        acme.id = Some(sovereign_db::schema::raw_to_thing("entity:acme").unwrap());
        let entities = vec![acme];
        let f = vec![finding(PiiKind::OrgName, "Acme", 0, 0.8)];
        let r = disambiguate(f, &entities, &[]);
        assert_eq!(r.linked[0].entity_id.as_deref(), Some("entity:acme"));
    }

    #[test]
    fn person_name_unmatched_proposes_person_entity() {
        let f = vec![finding(PiiKind::PersonName, "Charlie Newcomer", 0, 0.85)];
        let r = disambiguate(f, &[], &[]);
        assert_eq!(r.proposed_entities.len(), 1);
        assert_eq!(r.proposed_entities[0].kind, EntityKind::Person);
        assert_eq!(r.proposed_entities[0].name, "Charlie Newcomer");
        assert!(!r.proposed_entities[0].is_owned);
    }

    #[test]
    fn duplicate_unmatched_names_propose_once() {
        let f = vec![
            finding(PiiKind::PersonName, "Charlie Newcomer", 0, 0.85),
            finding(PiiKind::PersonName, "charlie newcomer", 30, 0.7),
        ];
        let r = disambiguate(f, &[], &[]);
        assert_eq!(r.proposed_entities.len(), 1);
    }

    // --- self-PII kinds pass through ---

    #[test]
    fn iban_finding_does_not_link_or_propose() {
        let f = vec![finding(PiiKind::Iban, "CH9300762011623852957", 0, 1.0)];
        let r = disambiguate(f, &[], &[]);
        assert!(r.linked[0].entity_id.is_none());
        assert!(r.proposed_entities.is_empty());
    }

    #[test]
    fn avs_finding_does_not_link_or_propose() {
        let f = vec![finding(PiiKind::Avs, "756.1234.5678.97", 0, 1.0)];
        let r = disambiguate(f, &[], &[]);
        assert!(r.linked[0].entity_id.is_none());
        assert!(r.proposed_entities.is_empty());
    }

    // --- ordering preserved ---

    #[test]
    fn linked_findings_preserve_input_order() {
        let f = vec![
            finding(PiiKind::Email, "a@x.com", 0, 1.0),
            finding(PiiKind::Phone, "555-123-4567", 10, 1.0),
            finding(PiiKind::Iban, "CH9300762011623852957", 30, 1.0),
        ];
        let r = disambiguate(f.clone(), &[], &[]);
        for (i, lf) in r.linked.iter().enumerate() {
            assert_eq!(lf.finding.start, f[i].start);
            assert_eq!(lf.finding.kind, f[i].kind);
        }
    }
}
