"""Independent validation of actual multi-provider checker observations."""
import argparse,hashlib,json
from importlib.metadata import version
from pathlib import Path
from jsonschema import Draft202012Validator
p=argparse.ArgumentParser()
for name in ('bundle','sources','compiler','schema','output'):p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
assert version('jsonschema')=='4.25.1'
b=json.loads(a.bundle.read_text());assert b['status']=='checked'
source_rows=[{'path':f.name,'sha256':sha(f)} for f in sorted(a.sources.iterdir()) if f.is_file()]
assert len(source_rows)==3
assert {(u['path'],u['export']['source_digest']) for u in b['units']}=={(r['path'],r['sha256']) for r in source_rows}
exports={u['export']['module_name']:u['export'] for u in b['units']}
c=exports['sample']['call_resolution_observations'];assert c['truncated'] is False
rows=c['calls'];assert len(rows)==2 and all(r['status']=='resolved' for r in rows)
schema=json.loads(a.schema.read_text());v=Draft202012Validator({'$schema':schema['$schema'],'$defs':schema['$defs'],'$ref':'#/$defs/checked_call_observation'})
for row in rows:
 v.validate(row)
 sig=row['target']['signature'];assert sig in exports[sig['module']]['purity_contracts']
actual={(r['target']['signature']['module'],tuple(r['target']['signature']['parameter_types'])) for r in rows}
assert actual=={('one',('Int',)),('two',('Bool',))},actual
report={'schema':'terlan.semantic.multi-provider-selection-evidence.v1','status':'passed','compiler_digest':sha(a.compiler),'artifact_sha256':sha(a.bundle),'schema_sha256':sha(a.schema),'sources':source_rows,'selected_targets':[{'module':m,'parameter_types':list(t)} for m,t in sorted(actual)],'validator':{'jsonschema':version('jsonschema')},'scope':'Two actual checked calls to the same imported local name from distinct providers. Only the accepted provider/signature is observed, agrees with its checked interface, and satisfies the public schema. No service or ABI proof.'}
with a.output.open('x') as out:json.dump(report,out,indent=2);out.write('\n')
print(json.dumps({'status':'passed','calls':len(rows)}))
