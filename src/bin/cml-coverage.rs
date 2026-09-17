use cml::coverage::CoverageLedger;

fn main() {
    print!("{}", CoverageLedger::pinned_submodule().to_lisp());
}
