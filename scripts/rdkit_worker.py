import multiprocessing

_H  =  1.007276
_Na = 22.989218
_K  = 38.963158
_Li =  6.015123
_NH4= 18.034164
_H2O= 18.010565
_NH3= 17.026549
_ACN= 41.026549
_FA = 46.005479

ADDUCT_TABLE = {
    "[M+H]+":            (1,   _H,            1),
    "[M+Na]+":           (1,   _Na,           1),
    "[M+K]+":            (1,   _K,            1),
    "[M+NH4]+":          (1,   _NH4,          1),
    "[M+Li]+":           (1,   _Li,           1),
    "[M+2H]2+":          (1,   2*_H,          2),
    "[M+H+Na]2+":        (1,   _H + _Na,      2),
    "[M+H+K]2+":         (1,   _H + _K,       2),
    "[M+2Na]2+":         (1,   2*_Na,         2),
    "[M+2K]2+":          (1,   2*_K,          2),
    "[M+2Na-H]2+":       (1,   2*_Na - _H,    2),
    "[M+3H]3+":          (1,   3*_H,          3),
    "[M+2H+Na]3+":       (1,   2*_H + _Na,    3),
    "[M+2H+K]3+":        (1,   2*_H + _K,     3),
    "[M+H+2Na]3+":       (1,   _H + 2*_Na,    3),
    "[2M+H]+":           (2,   _H,            1),
    "[2M+Na]+":          (2,   _Na,           1),
    "[2M+K]+":           (2,   _K,            1),
    "[2M+NH4]+":         (2,   _NH4,          1),
    "[2M+2H]2+":         (2,   2*_H,          2),
    "[2M-2H+H]+":        (2,  -_H,            1),
    "[2M-H2O+H]+":       (2,   _H - _H2O,    1),
    "[2M-2H2O+H]+":      (2,   _H - 2*_H2O,  1),
    "[3M+H]+":           (3,   _H,            1),
    "[3M+Na]+":          (3,   _Na,           1),
    "[3M+K]+":           (3,   _K,            1),
    "[3M+NH4]+":         (3,   _NH4,          1),
    "[M-H2O+H]+":        (1,   _H - _H2O,    1),
    "[M-2H2O+H]+":       (1,   _H - 2*_H2O,  1),
    "[M-3H2O+H]+":       (1,   _H - 3*_H2O,  1),
    "[M-4H2O+H]+":       (1,   _H - 4*_H2O,  1),
    "[M-5H2O+H]+":       (1,   _H - 5*_H2O,  1),
    "[M+H-H2O]+":        (1,   _H - _H2O,    1),
    "[M+H-2H2O]+":       (1,   _H - 2*_H2O,  1),
    "[M+H-3H2O]+":       (1,   _H - 3*_H2O,  1),
    "[M-H2O+Na]+":       (1,   _Na - _H2O,   1),
    "[M-H2O+NH4]+":      (1,   _NH4 - _H2O,  1),
    "[M-2H2O+NH4]+":     (1,   _NH4 - 2*_H2O,1),
    "[M-2H2O+Na]+":      (1,   _Na - 2*_H2O, 1),
    "[M-NH3+H]+":        (1,   _H - _NH3,    1),
    "[M+H-NH3]+":        (1,   _H - _NH3,    1),
    "[M-H2+H]+":         (1,   _H - 2*1.007825, 1),
    "[M-H2O+2H]2+":      (1,   2*_H - _H2O,  2),
    "[M-3H2O+2H]2+":     (1,   2*_H - 3*_H2O,2),
    "[M-2H2O+2H]2+":     (1,   2*_H - 2*_H2O,2),
    "[M+C2H3N+H]+":      (1,   _ACN + _H,    1),
    "[M+C2H3N+NH4]+":    (1,   _ACN + _NH4,  1),
    "[M+C2H3N+2H]2+":    (1,   _ACN + 2*_H,  2),
    "[M+3C2H3N+2H]+":    (1,   3*_ACN + 2*_H,1),
    "[M+C2H6OS+H]+":     (1,   79.021299 + _H - _H, 1),
    "[M+CH2O2+H]+":      (1,  47.013305,     1),
    "[M+CO2H2+H]+":      (1,  47.013305,     1),
    "[M+CH4O+H]+":       (1,  33.034164,     1),
    "[M-CH4O+H]+":       (1,   _H - 32.026215, 1),
    "[M]+":              (1,  -0.000549,      1),
    "[M]*+":             (1,  -0.000549,      1),
    "[M]2+":             (1,  -2*0.000549,    2),
    "[M]":               (1,   0.0,           1),
}

def calc_expected_mz(neutral_mass, precursor_type):
    if not precursor_type:
        return None
    entry = ADDUCT_TABLE.get(str(precursor_type).strip())
    if entry is None:
        return None
    mult, delta, charge = entry
    return (mult * neutral_mass + delta) / charge

def process_single(input_tuple):
    # input_tuple is (clean_mol, precursortype, precursormz)
    clean_mol, precursortype, precursormz_str = input_tuple
    
    try:
        precursormz = float(precursormz_str)
    except:
        precursormz = None

    import rdkit
    from rdkit import RDLogger
    RDLogger.DisableLog('rdApp.*')
    from rdkit import Chem
    from rdkit.Chem.rdMolDescriptors import CalcMolFormula, CalcExactMolWt
    from rdkit.Chem.Descriptors import MolWt
    from rdkit.Chem import rdmolops
    from rdkit.Chem.MolStandardize import rdMolStandardize
    
    transforms = {}
    try:
        if "InChI=" in clean_mol:
            mol = Chem.MolFromInchi(clean_mol)
        else:
            mol = Chem.MolFromSmiles(clean_mol)
            
        if mol is None:
            return (input_tuple, transforms)

        # =====================================================================
        # LOGIQUE DE RÉSOLUTION DES FRAGMENTS (SMILES contenant un '.')
        # =====================================================================
        # RDKit possède un outil classique pour retirer les sels (LargestFragmentChooser),
        # qui garde simplement le fragment avec le plus grand nombre d'atomes.
        #
        # Cependant, selon l'idée de Didier, en spectrométrie de masse, appliquer cette
        # règle aveuglément est dangereux : un SMILES avec un '.' peut représenter un
        # hétérodimère ou un complexe légitime qui est ionisé en un seul bloc.
        #
        # C'est pourquoi nous utilisons la vérité terrain du spectre (PRECURSORMZ) :
        # 1. On calcule la masse attendue du SMILES complet. Si elle correspond au PRECURSORMZ,
        #    c'est un complexe légitime, on ne touche à rien.
        # 2. Sinon, on calcule la masse de chaque fragment individuel. Si un fragment correspond
        #    exactement au PRECURSORMZ, c'est lui l'ion observé, le reste était un sel/solvant.
        # 3. En dernier recours (heuristique), si aucun fragment ne correspond parfaitement mais
        #    que le second fragment est très petit (<= 4 atomes lourds, ex: Cl, Na, H2O),
        #    on le retire en supposant que c'est un contre-ion.
        # =====================================================================
        if '.' in clean_mol and precursormz is not None:
            parts = clean_mol.split('.')
            fragments = []
            for part in parts:
                m = Chem.MolFromSmiles(part)
                if m:
                    fragments.append((m, m.GetNumHeavyAtoms(), CalcExactMolWt(m)))
            
            if fragments and sum(f[1] for f in fragments) > min(f[1] for f in fragments):
                mass_full = CalcExactMolWt(mol)
                mz_full = calc_expected_mz(mass_full, precursortype)
                
                # Check if full matches
                full_matches = mz_full is not None and abs(mz_full - precursormz) <= 0.02
                
                if not full_matches:
                    matching_frags = []
                    for (fmol, fn, fm) in fragments:
                        mz_f = calc_expected_mz(fm, precursortype)
                        if mz_f is not None and abs(mz_f - precursormz) <= 0.02:
                            matching_frags.append((fmol, fn, fm))
                            
                    if len(matching_frags) == 1:
                        mol = matching_frags[0][0]
                    elif len(matching_frags) > 1:
                        mol = max(matching_frags, key=lambda x: x[1])[0]
                    else:
                        frags_sorted = sorted(fragments, key=lambda x: x[1], reverse=True)
                        second_n = frags_sorted[1][1] if len(frags_sorted) > 1 else 0
                        if second_n <= 4:
                            mol = frags_sorted[0][0]
        
        # Charge neutralization
        try:
            charge = rdmolops.GetFormalCharge(mol)
            if charge != 0:
                uncharger = rdMolStandardize.Uncharger()
                mol_neutral = uncharger.uncharge(mol)
                if mol_neutral is not None and rdmolops.GetFormalCharge(mol_neutral) == 0:
                    mol = mol_neutral
        except:
            pass
            
        # Canonicalize using TautomerEnumerator
        try:
            enumerator = rdMolStandardize.TautomerEnumerator()
            mol = enumerator.Canonicalize(mol)
        except:
            pass

        try: transforms["INCHI"] = Chem.MolToInchi(mol)
        except: pass
        try: transforms["INCHIKEY"] = Chem.MolToInchiKey(mol)
        except: pass
        try: transforms["SMILES"] = Chem.MolToSmiles(mol, canonical=True)
        except: pass
        try: transforms["FORMULA"] = CalcMolFormula(mol)
        except: pass
        try: transforms["EXACTMASS"] = str(CalcExactMolWt(mol))
        except: pass
        try: transforms["AVERAGEMASS"] = str(MolWt(mol))
        except: pass
        
    except Exception as e:
        pass
        
    return (input_tuple, transforms)

def run_parallel(mols_list, progress_cb):
    # mols_list is a list of tuples: (clean_mol, precursortype, precursormz)
    results = {}
    cores = max(1, multiprocessing.cpu_count() - 1)
    
    # We must stringify the key to return it via PyO3 as a dict key.
    # PyO3 Dict handles string keys best.
    # We will format the key as: "clean_mol|precursortype|precursormz_str"
    with multiprocessing.Pool(processes=cores) as pool:
        for i, (input_tuple, transforms) in enumerate(pool.imap_unordered(process_single, mols_list, chunksize=100)):
            key = f"{input_tuple[0]}|{input_tuple[1]}|{input_tuple[2]}"
            results[key] = transforms
            if (i + 1) % 500 == 0 and progress_cb is not None:
                progress_cb(i + 1)
                
    return results
