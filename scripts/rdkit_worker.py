import multiprocessing

def process_single(clean_mol):
    import rdkit
    from rdkit import RDLogger
    RDLogger.DisableLog('rdApp.*')
    from rdkit import Chem
    from rdkit.Chem.rdMolDescriptors import CalcMolFormula
    from rdkit.Chem.Descriptors import ExactMolWt, MolWt
    
    transforms = {}
    try:
        if "InChI=" in clean_mol:
            mol = Chem.MolFromInchi(clean_mol)
        else:
            mol = Chem.MolFromSmiles(clean_mol)
            
        if mol is not None:
            try: transforms["INCHI"] = Chem.MolToInchi(mol)
            except: pass
            try: transforms["INCHIKEY"] = Chem.MolToInchiKey(mol)
            except: pass
            try: transforms["SMILES"] = Chem.MolToSmiles(mol)
            except: pass
            try: transforms["FORMULA"] = CalcMolFormula(mol)
            except: pass
            
            i_str = transforms.get("INCHI", "")
            s_str = transforms.get("SMILES", "")
            target_for_mass = i_str if i_str else s_str
            
            if "InChI=" in target_for_mass:
                mol_mass = Chem.MolFromInchi(target_for_mass)
            else:
                mol_mass = Chem.MolFromSmiles(target_for_mass)
                
            if mol_mass is not None:
                try: transforms["EXACTMASS"] = str(ExactMolWt(mol_mass))
                except: pass
                try: transforms["AVERAGEMASS"] = str(MolWt(mol_mass))
                except: pass
    except Exception:
        pass
        
    return (clean_mol, transforms)

def run_parallel(mols_list, progress_cb):
    results = {}
    cores = max(1, multiprocessing.cpu_count() - 1)
    
    with multiprocessing.Pool(processes=cores) as pool:
        for i, (mol, transforms) in enumerate(pool.imap_unordered(process_single, mols_list, chunksize=100)):
            results[mol] = transforms
            if (i + 1) % 500 == 0 and progress_cb is not None:
                progress_cb(i + 1)
                
    return results
