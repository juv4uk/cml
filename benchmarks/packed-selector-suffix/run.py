#!/usr/bin/env python3
"""cml#402 research-only packed selector suffix benchmark."""

from __future__ import annotations
import argparse, csv, json, os, platform, re, shutil, statistics, subprocess, tempfile
from pathlib import Path

DEPTHS = (8, 16, 32, 64, 128)

def checked(cmd):
    return subprocess.run(cmd, text=True, capture_output=True, check=True)

def path_bits(depth):
    return [((i * 7 + 5) ^ (i >> 2)) & 1 for i in range(depth)]

def byte_init(depth):
    return ", ".join(str(x) for x in path_bits(depth))

def packed_init(depth):
    bits = path_bits(depth)
    out=[]
    for base in range(0, depth, 8):
        b=0
        for j in range(8):
            if base+j < depth and bits[base+j]:
                b |= 1 << j
        out.append(b)
    return ", ".join(str(x) for x in out)

def unrolled(depth):
    lines=["    Node *p = root;"]
    for b in path_bits(depth):
        lines.append(f"    p = p->{'cdr' if b else 'car'};")
    lines.append("    return p;")
    return "\n".join(lines)

def generate_c():
    defs=[]
    funcs=[]
    cases=[]
    for d in DEPTHS:
        defs.append(f"static const uint8_t BYTE_{d}[{d}] = {{{byte_init(d)}}};")
        defs.append(f"static const uint8_t PACK_{d}[{(d+7)//8}] = {{{packed_init(d)}}};")
        funcs.append(f"__attribute__((noinline)) static Node *unrolled_{d}(Node *root) {{\n{unrolled(d)}\n}}\n")
        cases.append(f"case {d}: *byte_path=BYTE_{d}; *packed_path=PACK_{d}; return unrolled_{d};")
    return f'''#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Node {{ struct Node *car, *cdr; uint64_t tag; }} Node;
typedef Node *(*selector_fn)(Node *);

{os.linesep.join(defs)}

__attribute__((noinline)) static Node *identity(Node *p) {{
    __asm__ volatile("" : "+r"(p));
    return p;
}}

__attribute__((noinline)) static Node *byte_loop(Node *root, const uint8_t *path, size_t depth) {{
    Node *p=root;
    for(size_t i=0;i<depth;i++) p = path[i] ? p->cdr : p->car;
    return p;
}}

__attribute__((noinline)) static Node *packed_loop(Node *root, const uint8_t *path, size_t depth) {{
    Node *p=root;
    for(size_t i=0;i<depth;i++) {{
        uint8_t bit=(path[i >> 3] >> (i & 7)) & 1u;
        p = bit ? p->cdr : p->car;
    }}
    return p;
}}

{os.linesep.join(funcs)}

static selector_fn fixture(size_t depth, const uint8_t **byte_path, const uint8_t **packed_path) {{
    switch(depth) {{
        {' '.join(cases)}
        default: return NULL;
    }}
}}

static Node *build(size_t depth, const uint8_t *path, Node *nodes, Node *poison) {{
    for(size_t i=0;i<=depth;i++) {{ nodes[i].car=NULL; nodes[i].cdr=NULL; nodes[i].tag=i; }}
    for(size_t i=0;i<depth;i++) {{
        poison[i].car=&poison[i]; poison[i].cdr=&poison[i]; poison[i].tag=0xdead0000u+i;
        if(path[i]) {{ nodes[i].cdr=&nodes[i+1]; nodes[i].car=&poison[i]; }}
        else {{ nodes[i].car=&nodes[i+1]; nodes[i].cdr=&poison[i]; }}
    }}
    return &nodes[0];
}}

int main(int argc, char **argv) {{
    if(argc != 4) return 2;
    const char *mode=argv[1];
    size_t depth=strtoul(argv[2],NULL,10);
    uint64_t reps=strtoull(argv[3],NULL,10);
    const uint8_t *bp=NULL,*pp=NULL;
    selector_fn uf=fixture(depth,&bp,&pp);
    if(!uf || depth>128) return 3;
    Node nodes[129], poison[128];
    Node *root=build(depth,bp,nodes,poison), *expected=&nodes[depth];
    if(uf(root)!=expected || byte_loop(root,bp,depth)!=expected || packed_loop(root,pp,depth)!=expected) return 4;
    volatile uintptr_t sink=0;
    for(uint64_t i=0;i<reps;i++) {{
        Node *out;
        if(strcmp(mode,"base")==0) out=identity(root);
        else if(strcmp(mode,"unrolled")==0) out=uf(root);
        else if(strcmp(mode,"byte")==0) out=byte_loop(root,bp,depth);
        else if(strcmp(mode,"packed")==0) out=packed_loop(root,pp,depth);
        else return 5;
        sink ^= (uintptr_t)out;
    }}
    return sink==0x1234u ? 9 : 0;
}}
'''

def irefs(vg,binary,mode,depth,n):
    p=subprocess.run([vg,"--tool=cachegrind","--cache-sim=no","--branch-sim=no","--cachegrind-out-file=/dev/null",
                      str(binary),mode,str(depth),str(n)],text=True,capture_output=True,check=True)
    m=re.search(r"I\s+refs:\s*([\d,]+)",p.stderr)
    if not m: raise RuntimeError(p.stderr)
    return int(m.group(1).replace(",",""))

def sizes(nm,binary):
    p=checked([nm,"-S","--size-sort","--radix=d",str(binary)])
    out={}
    for line in p.stdout.splitlines():
        x=line.split()
        if len(x)>=4 and x[1].isdigit(): out[x[-1]]=int(x[1])
    return out

def version(cmd):
    try:
        p=checked(cmd); return (p.stdout or p.stderr).splitlines()[0].strip()
    except Exception: return "unknown"

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--out-dir",default="benchmarks/packed-selector-suffix/results/current")
    ap.add_argument("--calls",type=int,default=50000)
    ap.add_argument("--reps",type=int,default=3)
    a=ap.parse_args()
    gcc,vg,nm=map(shutil.which,("gcc","valgrind","nm"))
    if not all((gcc,vg,nm)): raise SystemExit("gcc, valgrind, nm required")
    repo=Path(__file__).resolve().parents[2]
    out=(repo/a.out_dir).resolve(); out.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="cml402-") as td:
        td=Path(td); src=td/"bench.c"; binary=td/"bench"
        src.write_text(generate_c())
        checked([gcc,"-O2","-std=gnu11","-fno-omit-frame-pointer","-o",str(binary),str(src)])
        sz=sizes(nm,binary)
        rows=[]
        for d in DEPTHS:
            vals={}
            for mode in ("base","unrolled","byte","packed"):
                samples=[irefs(vg,binary,mode,d,a.calls) for _ in range(a.reps)]
                vals[mode]=statistics.median(samples)
            base=vals["base"]
            for mode in ("unrolled","byte","packed"):
                desc=0 if mode=="unrolled" else d if mode=="byte" else (d+7)//8
                code=sz.get(f"unrolled_{d}",0) if mode=="unrolled" else sz.get(f"{mode}_loop",0)
                rows.append({
                    "depth":d,"candidate":mode,
                    "net_i_refs_per_call":f"{(vals[mode]-base)/a.calls:.3f}",
                    "code_bytes":code,"descriptor_bytes":desc,
                    "static_footprint_bytes":code+desc,
                    "primitive_steps":d,
                    "runtime_path_decode_ops":0 if mode=="unrolled" else d
                })
        with (out/"summary.tsv").open("w",newline="") as f:
            w=csv.DictWriter(f,fieldnames=list(rows[0]),delimiter="\t"); w.writeheader(); w.writerows(rows)
        prov={"git_sha":version(["git","-C",str(repo),"rev-parse","HEAD"]),
              "cpu":platform.processor() or version(["sh","-c","grep -m1 'model name' /proc/cpuinfo | cut -d: -f2-"]),
              "gcc":version([gcc,"--version"]),"valgrind":version([vg,"--version"]),
              "kernel":platform.release(),"calls":a.calls,"reps":a.reps,
              "note":"research-only packed suffix mechanism; Cachegrind cache/branch simulation disabled"}
        (out/"provenance.json").write_text(json.dumps(prov,indent=2)+"\n")
        print((out/"summary.tsv").read_text(),end="")

if __name__=="__main__": main()
