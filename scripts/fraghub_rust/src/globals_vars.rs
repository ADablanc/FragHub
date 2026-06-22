// src/globals_vars.rs

use once_cell::sync::Lazy;
use regex::Regex;

/// Expressions régulières compilées une seule fois pour tout le programme.
///
/// Pour un développeur Python : Si vous utilisez `re.match()` ou compilez une regex à l'intérieur
/// d'une boucle Python, cela ralentit tout. Ici, on utilise `Lazy` pour compiler chaque Regex 
/// au tout premier appel, puis on les garde en RAM. C'est l'un des secrets des 
/// performances exceptionnelles de ce parseur natif.

// =================================================== REGEX PATTERN ====================================================

// ============ Parsors regex ============

pub static IS_ADDUCT_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)m\]?(\-|\+)").unwrap());

pub static METADATA_STRIP_VALUE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^"|"$"#).unwrap());

pub static METADATA_FIELDS_NAME_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\W_]+|[\W_]+$").unwrap());

pub static METADATA_PATTERN_MGF: Lazy<Regex> = Lazy::new(|| Regex::new(r"([^:\n]*?)=\s*([^\n]*)(?:\n|$)").unwrap());

pub static METADATA_PATTERN_MSP: Lazy<Regex> = Lazy::new(|| Regex::new(r"([^:]*):(?: )?([^\n]*)(?:\n|$)").unwrap());

pub static COMPUTED_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)computed").unwrap());

pub static COMMENT_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)comment.*").unwrap());

pub static PEAK_LIST_SPLIT_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:^|\n)(-?\d+\.?\d*(?:[Ee][+-]?\d+)?)(?:\s+|:)(-?\d+[.,]?\d*(?:[Ee][+-]?\d+)?)").unwrap());

pub static PEAK_LIST_JSON_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(-?\d+\.?\d*(?:[Ee][+-]?\d+)?)(?:\s+|:|,|, )(-?\d+[.,]?\d*(?:[Ee][+-]?\d+)?)").unwrap());

pub static SUB_FIELDS_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(\S+?)="([^"]*)"|"(\w+?)=([^"]*)"|"([^"]*?)=([^"]*)"|(\S+?)=(\d+(?:[.,]\d*)?)|(\S+?)=(.*?)(?:;|\n|$)"#).unwrap());

// Version corrigée (Greedy, identique à Python) : Pour le MGF (Même chose, on remet [\s\S]* et le =)
pub static METADATA_PEAK_LIST_SPLIT_PATTERN_MGF: Lazy<Regex> = Lazy::new(|| Regex::new(r"([\s\S]*=.*[0-9]*\n)((?:(?:-?\d+[.,]?\d*(?:[Ee][+-]?\d+)?)(?:\s+|:)(?:-?\d+[.,]?\d*(?:[Ee][+-]?\d+)?).*(?:\n|$))*)").unwrap());

// Version corrigée (Greedy, identique à Python) : Pour le MSP (On remet [\s\S]* au début et on supprime (?s))
pub static METADATA_PEAK_LIST_SPLIT_PATTERN_MSP: Lazy<Regex> = Lazy::new(|| Regex::new(r"([\s\S]*:.*[0-9]*\n)((?:(?:-?\d+[.,]?\d*(?:[Ee][+-]?\d+)?)(?:\s+|:)(?:-?\d+[.,]?\d*(?:[Ee][+-]?\d+)?).*(?:\n|$))*)").unwrap());

// ======================================

// ===== normalizers regex pattern ======

pub static INDIGO_SMILES_CORRECTION_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\|[\s\S]*").unwrap());

pub static SUB_SIGNE_END_ADDUCT_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\-|\+)$").unwrap());

pub static SUB_ADDUCT_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\(|\)|(.*\[)|(\]([\d\+\-\*]*)?)").unwrap());

pub static FLOAT_CHECK_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(-?\d+[.,]?\d*(?:[Ee][+-]?\d+)?)").unwrap());

pub static MS_LEVEL_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)(?:ms)?(\d)").unwrap());

pub static IONMODE_POS_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^p|^\+|^pos").unwrap());

pub static IONMODE_NEG_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^n|^\-|^neg").unwrap());

pub static REPAIR_INCHI_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^(?:inchi=)?").unwrap());

// Match inchi
pub static INCHI_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)InChI=.*|/[0-9A-Z]*/").unwrap());

// Match smiles
pub static SMILES_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)[^J][a-z0-9@+\-\[\]\(\)\\/%=#$]{6,}").unwrap());

// Match inchikey or short inchikey
pub static INCHIKEY_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)([A-Z]{14}-[A-Z]{10}-[NO])|([A-Z]{14})").unwrap());

pub static IN_SILICO_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)in.silico|insilico|predicted|theoretical|Annotation\.level\.3").unwrap());

pub static RETENTION_TIME_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)(-?\d+[.,]?\d*(?:[Ee][+-]?\d+)?)(?:\W)?(m|min|minute|minutes|s|sec|second|seconds|ms|millisecond|milliseconds)(?:\W)?").unwrap());

// Regex optimisée pour capturer les modes d'ionisation avec détection des limites de mots (\b)
pub static IONIZATION_MODE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(APCI|ACPI|APPI|EI|ESI|FAB|MALDI)\b").unwrap());

pub static EMPTY_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:(?:CCS:( .*)?)|(?:\$:00in-source( .*)?)|(?:0( .*)?)|(?:0\.0( .*)?)|(?:)|(?:na( .*)?)|(?:n/a( .*)?)|(?:nan( .*)?)|(?:unknown( .*)?)|(?:unknow( .*)?)|(?:none( .*)?)|(?:\?( .*)?)|(?:unk( .*)?)|(?:x( .*)?))$").unwrap()
});

// =====================================

// ======================================================================================================================
// VARIABLES AJOUTÉES CÔTÉ RUST (N'existaient pas dans globals_vars.py)
// ======================================================================================================================

pub static GC_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bGC\b").unwrap());

// Regex pour extraire Collision Energy du Compound_Name (ex: GNPS, supporte 20-30, 102040/13334, 10 20 30)
pub static GNPS_COLLISION_ENERGY_EXTRACT_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)(?:\W|_)*collision[-\s_]?energy[\s:=]*(\d+(?:[\s.,\-/]+\d+)*\s*(?:eV|V)?)(?:\W|_)*").unwrap());