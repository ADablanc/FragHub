// En-tête de la passerelle C++ vers RDKit (voir shim.cc)
#pragma once
#include "rust/cxx.h"

namespace fraghub {
struct MolResult;
void init_rdkit();
MolResult process_single(rust::Str clean_mol, rust::Str ptype, double precursormz, bool has_mz,
                         bool adduct_known, int adduct_mult, double adduct_delta, int adduct_charge);
double exact_mass(rust::Str mol, bool is_inchi);
void set_time_limit(double max_secs);
bool take_timed_out();
}  // namespace fraghub
