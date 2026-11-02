"""Independent schema oracle for actual checked binding type observations."""
import argparse, hashlib, json
from importlib.metadata import version
from pathlib import Path
from jsonschema import Draft202012Validator
parser=argparse.ArgumentParser()
for name in ('artifact','source','schema','compiler','output'):
    parser.add_argument('--'+name, type=Path, required=True)
args=parser.parse_args()
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
a=json.loads(args.artifact.read_text());s=json.loads(args.schema.read_text())
assert version('jsonschema')=='4.25.1'
v=Draft202012Validator({'$schema':s['$schema'],'$defs':s['$defs'],'$ref':'#/$defs/inferred_binding_type'})
assert a['status']=='checked' and not a['binding_type_observations']['truncated']
assert a['source_digest']==sha(args.source)
rows=a['binding_type_observations']['bindings'];cases=[]
expected={'tuple_local':'Tuple','list_local':'List','map_local':'Map','named_local':'Named','array_local':'Array','generic_local':'Var','function_local':'Function'}
for name,kind in expected.items():
    matches=[r for r in rows if r['binding']['name']==name]
    assert len(matches)==1,(name,matches)
    row=matches[0];v.validate(row)
    assert row['binding'] in a['core']['binding_identities']['bindings']
    assert row['module']==a['core']['module']
    assert kind in row['compiler_type'],(name,row)
    cases.append({'name':name,'status':'passed','type_constructor':kind,'binding_id':row['binding']['id']})
report={'schema':'terlan.semantic.binding-type-schema-evidence.v1','status':'passed','cases':cases,'source_sha256':sha(args.source),'artifact_sha256':sha(args.artifact),'schema_sha256':sha(args.schema),'compiler_digest':sha(args.compiler),'validator':{'jsonschema':version('jsonschema')},'scope':'Seven real checked inferred binding type constructors, exact Core lexical joins and independent recursive public-schema validation. No service or ownership proof.'}
with args.output.open('x') as f:json.dump(report,f,indent=2);f.write('\n')
print(json.dumps({'status':'passed','cases':len(cases)}))
