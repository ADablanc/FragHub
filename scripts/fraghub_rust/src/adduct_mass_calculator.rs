// src/chemistry_engine.rs
use std::collections::HashMap;
use lazy_static::lazy_static;
use regex::Regex;

// Masses monoisotopiques des éléments (en Daltons)
lazy_static! {
    static ref MONOISOTOPIC: HashMap<&'static str, f64> = {
        let mut m = HashMap::new();
        m.insert("H", 1.00782503207);
        m.insert("C", 12.0);
        m.insert("N", 14.0030740048);
        m.insert("O", 15.99491461956);
        m.insert("P", 30.97376163);
        m.insert("S", 31.97207100);
        m.insert("F", 18.99840322);
        m.insert("Cl", 34.96885268);
        m.insert("Br", 78.91833710);
        m.insert("I", 126.904473);
        m.insert("Si", 27.9769265325);
        m.insert("B", 11.0093054);
        m.insert("Se", 79.9165218);
        m.insert("Li", 7.01600455);
        m.insert("Na", 22.9897692809);
        m.insert("K", 38.96370668);
        m.insert("Ca", 39.96259098);
        m.insert("Mg", 23.98504170);
        m.insert("Fe", 55.9349375);
        m.insert("Zn", 63.92914201);
        m.insert("Cu", 62.92959772);
        m.insert("Mn", 54.9380451);
        m.insert("Co", 58.9331950);
        m
    };

    static ref ELEM_RE: Regex = Regex::new(r"([A-Z][a-z]?)(\d*)").unwrap();
    static ref CHARGED_RE: Regex = Regex::new(r"^\[([^\]]+)\](\d*)([+-])$").unwrap();
    static ref ADDUCT_RE: Regex = Regex::new(r"^\[(\d*)M((?:[+-][^]]+)*)\](\d*)(\*?[+-])$").unwrap();
    static ref MOD_SPLIT_RE: Regex = Regex::new(r"([+-])(\d*)([A-Z][A-Za-z0-9]*)").unwrap();
}

pub const ELECTRON_MASS: f64 = 0.000548579909;

/// Convertit une formule brute en dictionnaire d'atomes.
pub fn parse_formula(formula: &str) -> HashMap<String, i32> {
    if formula.is_empty() || formula == "nan" || formula == "NOT FOUND" {
        return HashMap::new();
    }
    
    let raw = if let Some(caps) = CHARGED_RE.captures(formula) {
        caps.get(1).map_or(formula, |m| m.as_str())
    } else {
        formula
    };

    let mut counts = HashMap::new();
    for cap in ELEM_RE.captures_iter(raw) {
        let elem = cap[1].to_string();
        let cnt_str = &cap[2];
        let cnt = if cnt_str.is_empty() { 1 } else { cnt_str.parse::<i32>().unwrap_or(1) };
        *counts.entry(elem).or_insert(0) += cnt;
    }
    counts
}

/// Convertit un dictionnaire d'atomes en chaîne de formule brute (convention Hill)
pub fn formula_to_str(counts: &HashMap<String, i32>) -> String {
    if counts.is_empty() {
        return String::new();
    }
    
    let mut parts = Vec::new();
    for elem in ["C", "H"] {
        if let Some(&cnt) = counts.get(elem) {
            if cnt > 0 {
                if cnt > 1 {
                    parts.push(format!("{}{}", elem, cnt));
                } else {
                    parts.push(elem.to_string());
                }
            }
        }
    }
    
    let mut other_elems: Vec<&String> = counts.keys().filter(|&k| k != "C" && k != "H").collect();
    other_elems.sort();
    
    for elem in other_elems {
        if let Some(&cnt) = counts.get(elem) {
            if cnt > 0 {
                if cnt > 1 {
                    parts.push(format!("{}{}", elem, cnt));
                } else {
                    parts.push(elem.to_string());
                }
            }
        }
    }
    
    parts.join("")
}

/// Parse une formule au format crochet et retourne sa charge nette
pub fn formula_net_charge(formula: &str) -> i32 {
    if let Some(caps) = CHARGED_RE.captures(formula) {
        let charge_str = caps.get(2).map_or("", |m| m.as_str());
        let charge = if charge_str.is_empty() { 1 } else { charge_str.parse::<i32>().unwrap_or(1) };
        let sign = caps.get(3).map_or("+", |m| m.as_str());
        if sign == "+" { charge } else { -charge }
    } else {
        0
    }
}

pub fn format_charged_formula(formula_str: &str, charge_abs: i32, sign: &str) -> String {
    if formula_str.is_empty() {
        return String::new();
    }
    let suffix = if charge_abs == 1 {
        sign.to_string()
    } else {
        format!("{}{}", charge_abs, sign)
    };
    format!("[{}]{}", formula_str, suffix)
}

/// Parse un adduct et retourne (n_mol, mods_str, charge_abs, sign)
pub fn parse_precursor_type(pt: &str) -> Option<(i32, String, i32, String)> {
    let pt = pt.trim();
    if let Some(caps) = ADDUCT_RE.captures(pt) {
        let n_mol_str = caps.get(1).map_or("", |m| m.as_str());
        let n_mol = if n_mol_str.is_empty() { 1 } else { n_mol_str.parse::<i32>().unwrap_or(1) };
        
        let mods_str = caps.get(2).map_or("", |m| m.as_str()).to_string();
        
        let charge_str = caps.get(3).map_or("", |m| m.as_str());
        let sign_str = caps.get(4).map_or("+", |m| m.as_str());
        
        let sign_clean = sign_str.replace("*", "");
        let mut charge = if charge_str.is_empty() { 1 } else { charge_str.parse::<i32>().unwrap_or(1) };
        
        let pos = sign_clean.matches('+').count() as i32;
        let neg = sign_clean.matches('-').count() as i32;
        charge *= pos - neg;
        
        let sign = if charge > 0 { "+" } else { "-" }.to_string();
        Some((n_mol, mods_str, charge.abs(), sign))
    } else {
        None
    }
}

pub fn exact_mass_from_counts(counts: &HashMap<String, i32>) -> f64 {
    let mut mass = 0.0;
    for (e, &n) in counts {
        if let Some(&w) = MONOISOTOPIC.get(e.as_str()) {
            mass += w * (n as f64);
        }
    }
    mass
}

pub fn apply_adduct_to_formula(base_formula: &str, precursor_type: &str) -> String {
    if base_formula.is_empty() || precursor_type.is_empty() {
        return String::new();
    }
    
    let mol_charge = formula_net_charge(base_formula);
    
    if precursor_type == "[M]" || precursor_type == "[M]*+" || precursor_type == "[M]+" {
        let total_charge = if mol_charge > 0 { mol_charge } else { 1 };
        let f_str = formula_to_str(&parse_formula(base_formula));
        return format_charged_formula(&f_str, total_charge, "+");
    }
    
    if let Some((n_mol, mods_str, adduct_charge, sign)) = parse_precursor_type(precursor_type) {
        let mut counts = HashMap::new();
        for (elem, &cnt) in parse_formula(base_formula).iter() {
            *counts.entry(elem.clone()).or_insert(0) += cnt * n_mol;
        }
        
        let mut effective_mods = mods_str;
        if mol_charge == -1 {
            effective_mods = format!("+H{}", effective_mods);
        }
        
        for cap in MOD_SPLIT_RE.captures_iter(&effective_mods) {
            let op = &cap[1];
            let coeff_str = &cap[2];
            let coeff = if coeff_str.is_empty() { 1 } else { coeff_str.parse::<i32>().unwrap_or(1) };
            let fragment = &cap[3];
            
            for (elem, &cnt) in parse_formula(fragment).iter() {
                let change = if op == "+" { cnt * coeff } else { -cnt * coeff };
                *counts.entry(elem.clone()).or_insert(0) += change;
            }
        }
        
        counts.retain(|_, &mut v| v > 0);
        let f_str = formula_to_str(&counts);
        
        let h_compensation = if mol_charge == -1 { 1 } else { 0 };
        let total_charge = mol_charge * n_mol + adduct_charge + h_compensation;
        
        if total_charge <= 0 {
            return String::new();
        }
        
        return format_charged_formula(&f_str, total_charge, &sign);
    }
    
    String::new()
}

pub fn compute_mz(precursor_formula: &str) -> Option<f64> {
    if precursor_formula.is_empty() {
        return None;
    }
    
    if let Some(caps) = CHARGED_RE.captures(precursor_formula) {
        let formula_str = caps.get(1).map_or("", |m| m.as_str());
        let charge_str = caps.get(2).map_or("", |m| m.as_str());
        let sign = caps.get(3).map_or("+", |m| m.as_str());
        
        let charge_abs = if charge_str.is_empty() { 1 } else { charge_str.parse::<i32>().unwrap_or(1) };
        if charge_abs <= 0 {
            return None;
        }
        
        let counts = parse_formula(formula_str);
        if counts.is_empty() {
            return None;
        }
        
        let neutral_mass = exact_mass_from_counts(&counts);
        let mz = if sign == "+" {
            (neutral_mass - (charge_abs as f64) * ELECTRON_MASS) / (charge_abs as f64)
        } else {
            (neutral_mass + (charge_abs as f64) * ELECTRON_MASS) / (charge_abs as f64)
        };
        
        Some((mz * 1_000_000.0).round() / 1_000_000.0) // Arrondi à 6 décimales
    } else {
        None
    }
}
