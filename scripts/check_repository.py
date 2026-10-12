"""Stdlib gate: source/privacy boundaries, local links, SVGs, schemas and skills."""
import json
import re
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ALLOWED = {'.agents','.github','.gitattributes','.gitignore','AGENTS.md','README.md','LICENSE','CONTRIBUTING.md','SECURITY.md','CHANGELOG.md','Makefile','Cargo.toml','Cargo.lock','rust-toolchain.toml','crates','resources','gui','examples','providers','docs','scripts','tests'}
FORBIDDEN = [b'/Users/', b'-----BEGIN PRIVATE KEY-----', b'-----BEGIN OPENSSH PRIVATE KEY-----']

def source_paths():
    result=subprocess.run(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT,capture_output=True,check=True)
    return sorted({ROOT/p.decode() for p in result.stdout.split(b'\0') if p and ((ROOT/p.decode()).is_file() or (ROOT/p.decode()).is_symlink())})

def check():
    paths=source_paths();links=0
    for path in paths:
        name=path.relative_to(ROOT)
        if name.parts[0] not in ALLOWED or path.suffix in {'.jhb','.bundle','.sqlite','.pyc','.pyo','.task','.tflite','.safetensors','.litertlm'}:
            raise ValueError('Private/runtime file in source: '+str(name))
        if path.suffix=='.pdf' and name!=Path('examples/cv/example.pdf'):
            raise ValueError('Only the audited fictional CV PDF belongs in source: '+str(name))
        if path.is_symlink():
            raise ValueError('Source uses regular portable files; unexpected symlink: '+str(name))
        data=path.read_bytes()
        if name!=Path('scripts/check_repository.py') and any(needle in data for needle in FORBIDDEN):
            raise ValueError('Private path/key content in generic source: '+str(name))
        if path.suffix=='.svg':ET.fromstring(data)
        if path.suffix=='.json':json.loads(data)
        if path.suffix=='.md':
            for raw in re.findall(r'\]\(([^)]+)\)',data.decode()):
                target=raw.split('#',1)[0].strip('<>')
                if not target or re.match(r'[a-z][a-z0-9+.-]*:',target):continue
                if not (path.parent/target).exists():raise ValueError('Broken local link in '+str(name)+': '+target)
                links+=1
    skills=list((ROOT/'resources/skills').glob('*/SKILL.md')) + list((ROOT/'.agents/skills').glob('*/SKILL.md'))
    for path in skills:
        parts=path.read_text(encoding="utf-8").split('---',2)
        if len(parts)!=3:raise ValueError('Missing skill frontmatter: '+str(path))
        front=parts[1]
        if not re.search(r'(?m)^name:\s*'+re.escape(path.parent.name)+r'\s*$',front) or not re.search(r'(?m)^description:\s*\S',front):
            raise ValueError('Invalid skill metadata: '+path.parent.name)
    manifest=(ROOT/'Cargo.toml').read_text(encoding="utf-8")
    if 'publish = false' not in manifest:raise ValueError('Public publication must stay disabled')
    print(f'PASS generic source: {len(paths)} files, {links} links, {len(skills)} skills, schemas/SVG and private-data boundary')
if __name__=='__main__':check()
