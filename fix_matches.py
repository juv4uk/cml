import re
files = ['src/c_backend.rs', 'src/compiler.rs', 'src/compute.rs', 'src/x86_freestanding.rs']

for f in files:
    with open(f, 'r') as file:
        content = file.read()
    
    # find Quoted::DottedList(...) => ..., or similar and append _ => todo!()
    content = re.sub(r'(Quoted::DottedList\([^)]*\)\s*=>\s*\{[^}]*\})', r'\1,\n            _ => todo!("extended"),', content, flags=re.DOTALL)
    
    # for compute.rs Ir
    content = re.sub(r'(Ir::Prim\s*\{\s*op,\s*args\s*\}\s*=>\s*\[[^\]]*\])', r'\1,\n        _ => todo!("extended"),', content, flags=re.DOTALL)
    
    with open(f, 'w') as file:
        file.write(content)
