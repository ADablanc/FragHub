// src/normalizer/normalize_name.rs
use std::collections::HashMap;

/// Retire du nom ce qui n'est pas le nom de la molécule, sans perdre l'information.
///
/// - GNPS écrit parfois l'énergie de collision dans le nom : "Cholic acid CollisionEnergy:102040". Elle est
///   retirée du nom et mise dans COLLISIONENERGY si ce champ est vide (telle qu'elle est écrite : sa mise en
///   forme sera traitée à part). Avant, seul le JSON GNPS le faisait ; en MSP et MGF, elle restait dans le nom.
/// - MS-DIAL (MSMS-Public) écrit parfois l'énergie de collision et l'InChIKey en fin de nom, séparées par « ; » :
///   "Piperine; CE0; MXXWOMGUGJBKIW-YPCIICBESA-N" -> "Piperine", "Betaine; CE30; CE10; KWIU…-N" -> "Betaine"
///   (énergies "30, 10" mises dans COLLISIONENERGY s'il est vide ; un "CE0" ne dit rien et n'est pas repris).
///   Seuls les derniers morceaux qui sont exactement une InChIKey ou "CE" suivi d'un nombre sont retirés :
///   deux noms séparés par « ; » ("Isosaxalin; Heraclenol") restent tels quels, et le premier morceau est
///   toujours gardé.
pub fn clean_name_suffixes(mut metadata_dict: HashMap<String, String>) -> HashMap<String, String> {
    let Some(name) = metadata_dict.get("NAME").cloned() else { return metadata_dict };
    let mut energies: Vec<String> = Vec::new();
    let mut name_out = name.clone();
    if let Some(caps) = crate::globals_vars::GNPS_COLLISION_ENERGY_EXTRACT_PATTERN.captures(&name) {
        if let Some(e) = caps.get(1) {
            let real = crate::globals_vars::GNPS_COLLISION_ENERGY_EXTRACT_PATTERN.replace(&name, " ").trim().to_string();
            if !real.is_empty() {
                energies.push(e.as_str().trim().to_string());
                name_out = real;
            }
        }
    }
    let (cleaned, ce) = strip_suffixes(&name_out);
    energies.extend(ce);
    if cleaned != name {
        metadata_dict.insert("NAME".to_string(), cleaned);
    }
    let current = metadata_dict.get("COLLISIONENERGY").map(|s| s.trim().to_string()).unwrap_or_default();
    let energies: Vec<String> = energies.into_iter().filter(|e| !crate::globals_vars::EMPTY_PATTERN.is_match(e)).collect();
    if !energies.is_empty() && crate::globals_vars::EMPTY_PATTERN.is_match(&current) {
        metadata_dict.insert("COLLISIONENERGY".to_string(), energies.join(", "));
    }
    metadata_dict
}

/// Nom sans ses derniers morceaux InChIKey / "CEnn" ; énergies trouvées, dans l'ordre du nom.
fn strip_suffixes(name: &str) -> (String, Vec<String>) {
    let mut end = name.trim_end();
    let mut energies = Vec::new();
    while let Some(cut) = end.rfind(';') {
        let last = end[cut + 1..].trim();
        let head = end[..cut].trim_end();
        if head.is_empty() || !(is_inchikey(last) || is_collision_energy(last)) {
            break;
        }
        if is_collision_energy(last) {
            energies.insert(0, last[2..].to_string());
        }
        end = head;
    }
    let out = if end.len() == name.trim_end().len() { name.to_string() } else { end.to_string() };
    (out, energies)
}

fn is_inchikey(s: &str) -> bool {
    crate::globals_vars::INCHIKEY_FULL_PATTERN.is_match(s)
}

fn is_collision_energy(s: &str) -> bool {
    let rest = match s.strip_prefix("CE") { Some(r) => r, None => return false };
    !rest.is_empty() && rest.replace(',', ".").parse::<f64>().map_or(false, |x| x.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(n: &str) -> String {
        let mut m = HashMap::new();
        m.insert("NAME".to_string(), n.to_string());
        clean_name_suffixes(m)["NAME"].clone()
    }

    fn name_ce(n: &str, ce: &str) -> (String, String) {
        let mut m = HashMap::new();
        m.insert("NAME".to_string(), n.to_string());
        m.insert("COLLISIONENERGY".to_string(), ce.to_string());
        let m = clean_name_suffixes(m);
        (m["NAME"].clone(), m["COLLISIONENERGY"].clone())
    }

    #[test]
    fn energie_du_nom_gardee_pour_tous_les_formats() {
        // GNPS (même règle qu'avant en JSON, maintenant aussi en MSP et MGF)
        assert_eq!(name_ce("Cholic acid CollisionEnergy:102040", ""), ("Cholic acid".into(), "102040".into()));
        assert_eq!(name_ce("Cholic acid CollisionEnergy:102040", "35"), ("Cholic acid".into(), "35".into()));
        // MS-DIAL : énergies en fin de nom
        assert_eq!(name_ce("Betaine; CE30; CE10; KWIUHFFTVRNATP-UHFFFAOYSA-N", ""), ("Betaine".into(), "30, 10".into()));
        // "CE0" ne dit rien ; un champ « vide » écrit (0) est remplacé par une vraie valeur
        assert_eq!(name_ce("Piperine; CE0; MXXWOMGUGJBKIW-YPCIICBESA-N", ""), ("Piperine".into(), "".into()));
        assert_eq!(name_ce("Truc; CE20", "0"), ("Truc".into(), "20".into()));
    }

    #[test]
    fn energie_et_inchikey_retirees_du_nom() {
        // Noms réels de MSMS-Public (MS-DIAL)
        assert_eq!(name("Piperine; CE0; MXXWOMGUGJBKIW-YPCIICBESA-N"), "Piperine");
        assert_eq!(name("Betaine; CE30; CE10; KWIUHFFTVRNATP-UHFFFAOYSA-N"), "Betaine");
        assert_eq!(name("Truc; MXXWOMGUGJBKIW-YPCIICBESA-N"), "Truc");
        assert_eq!(name("Truc; CE12.5"), "Truc");
    }

    #[test]
    fn autres_noms_inchanges() {
        assert_eq!(name("Isosaxalin; Heraclenol"), "Isosaxalin; Heraclenol");
        assert_eq!(name("Chrysin"), "Chrysin");
        assert_eq!(name("PC 34:1; CE"), "PC 34:1; CE");
        assert_eq!(name("Acide; CEtone"), "Acide; CEtone");
        // Le nom n'est jamais vidé
        assert_eq!(name("MXXWOMGUGJBKIW-YPCIICBESA-N"), "MXXWOMGUGJBKIW-YPCIICBESA-N");
        assert_eq!(name("CE10; MXXWOMGUGJBKIW-YPCIICBESA-N"), "CE10");
    }
}
