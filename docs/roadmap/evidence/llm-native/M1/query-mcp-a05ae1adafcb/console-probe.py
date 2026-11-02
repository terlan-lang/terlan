import os,subprocess,selectors,json
from pathlib import Path
root=Path.cwd()
err=open('/tmp/terlan-stdio-probe-live-2.stderr','wb')
p=subprocess.Popen([str(root/'target/debug/terlc'),'run','/tmp/terlan-stdio-probe.terls','--cache-dir','/tmp/terlan-stdio-probe-cache-3'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,env=dict(os.environ,TERLAN_RUN_STDIO='1'))
s=selectors.DefaultSelector();s.register(p.stdout,selectors.EVENT_READ);rows=[]
try:
    for value in [b'first\n',b'second\r\n']:
        p.stdin.write(value);p.stdin.flush();assert s.select(90),'No response while stdin remained open'
        reply=p.stdout.readline();assert reply==value.rstrip(b'\r\n')+b'\n',repr(reply)
        rows.append({'sent':value.decode(),'received':reply.decode(),'stdin_open':True})
    p.stdin.close();assert p.wait(timeout=15)==0
    report={'status':'passed','cases':rows,'exit_code':p.returncode}
except Exception as error:
    p.kill();p.wait();report={'status':'failed','error':str(error),'cases':rows}
finally:err.close()
Path('/tmp/terlan-stdio-probe-live-2.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report));raise SystemExit(0 if report['status']=='passed' else 1)
