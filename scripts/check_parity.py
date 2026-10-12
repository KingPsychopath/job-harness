"""Cross-language compatibility gates. Synthetic workspaces only; stdlib Python."""
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
ORACLE = ROOT / 'tests/oracle'
sys.path.insert(0, str(ORACLE if ORACLE.exists() else ROOT))
from job_harness.service import Harness
from job_harness.ledger import Ledger
from job_harness.contracts import SPECS
from job_harness.bundles import export_bundle, import_bundle
from job_harness.pipeline import build

BIN = Path(os.environ.get('JOB_HARNESS_BINARY', ROOT / 'target/debug/job-harness')).resolve()

def native(root, name, args):
    p = subprocess.run([str(BIN), '--root', str(root), 'call', name], input=json.dumps(args), text=True, capture_output=True)
    result=json.loads(p.stdout)
    if result['ok'] != (p.returncode==0):
        raise AssertionError('CLI exit/envelope mismatch: '+name)
    return result

def command(root, *args):
    p=subprocess.run([str(BIN), '--root', str(root), *map(str,args)],text=True,capture_output=True)
    result=json.loads(p.stdout)
    if not result['ok']: raise AssertionError((args,result))
    return result['data']

def clean(value):
    if isinstance(value,dict):
        return {k:clean(v) for k,v in value.items() if k not in {'cli_fallback','toolkit'}}
    if isinstance(value,list): return [clean(v) for v in value]
    return value

def compare(h, root, name, args):
    old=h.call(name,args);new=native(root,name,args)
    if old['ok'] != new['ok']: raise AssertionError((name,old,new))
    if not old['ok']:
        if old['error']['code'] != new['error']['code']:raise AssertionError((name,old,new))
    elif clean(old['data']) != clean(new['data']):
        raise AssertionError((name,old['data'],new['data']))
    return old

def fixture(root):
    for rel in ['applications','context/profile','output','config']: (root/rel).mkdir(parents=True,exist_ok=True)
    (root/'config/career-ledger.json').write_text(json.dumps({'schema_version':1,'timezone':'UTC','primary_cv':'output/cv.pdf'}))
    (root/'config/career-workflows.json').write_bytes((ROOT/'resources/workflows.json').read_bytes())
    (root/'applications/acme.md').write_text('# Acme — Engineer\n\n- Stage: saved\n- Source URL: https://jobs.example.test/acme/123?utm_source=old\n')
    (root/'applications/approved-answers.md').write_text('# Answers\n\n## Experience\nSynthetic approved experience answer.\n')
    (root/'context/profile/application-decisions.md').write_text('# Decisions\n\n## Experience\nSynthetic source; not a real candidate fact.\n')
    (root/'output/cv.pdf').write_bytes(b'%PDF-1.7\nsynthetic\n')
    (root/'applications/answers.json').write_text('{"example": true, "number": 1e-07, "large": 1e20}')

with tempfile.TemporaryDirectory(prefix='harness-parity-') as directory:
    root=(Path(directory)/'source');fixture(root);h=Harness(root)
    count=0
    for name in SPECS:
        compare(h,root,name,{'unexpected_field':True});count+=1
    for name,args in [('list_operations',{}),('inspect_tools',{'names':['find_application'],'include_schemas':True}),('read_record',{'path':'applications/approved-answers.md'}),('get_approved_answer',{'query':'experience'}),('find_application',{'query':'Acme'}),('check_duplicate',{'company':'Acme','vacancy_url':'https://jobs.example.test/acme/123'}),('reconcile_tracker',{}),('bootstrap',{'operation':'reconcile','capabilities':[]})]:
        compare(h,root,name,args);count+=1
    run=compare(h,root,'start_run',{'operation':'apply','scope':{'sources':['employer'],'max_submissions':2,'dry_run':False},'user_request':'Synthetic migration proof','idempotency_key':'parity:run'})['data']['run_id'];count+=1
    packet_args={'run_id':run,'record':'applications/acme.md','company':'Acme','vacancy_url':'https://jobs.example.test/acme/123','source':'employer','posting_id':'123','cv_path':'output/cv.pdf','answers_path':'applications/answers.json','idempotency_key':'parity:packet'}
    packet=compare(h,root,'prepare_packet',packet_args)['data']['packet_id'];count+=1
    compare(h,root,'get_packet',{'packet_id':packet});count+=1
    attempt=compare(h,root,'reserve_submission',{'run_id':run,'packet_id':packet,'idempotency_key':'parity:reserve'})['data']['attempt_id'];count+=1
    begun=native(root,'begin_submission',{'attempt_id':attempt,'idempotency_key':'parity:begin'})
    assert begun['ok'] and begun['data']['ready_for_single_submit']
    replay=h.call('begin_submission',{'attempt_id':attempt,'idempotency_key':'parity:begin'})
    assert replay['ok'] and not replay['data']['ready_for_single_submit'];count+=2
    compare(h,root,'finish_submission',{'attempt_id':attempt,'outcome':'outcome_unknown','summary':'Synthetic uncertain provider outcome','evidence':['synthetic:observation'],'idempotency_key':'parity:unknown'});count+=1
    duplicate=native(root,'check_duplicate',{'company':'Acme','vacancy_url':'https://jobs.example.test/acme/other'})
    assert duplicate['ok'] and duplicate['data']['blocked'];count+=1
    # Unknown historical fields are carried through mutations and transfers.
    state=root/'applications/operations/state.json';v=json.loads(state.read_text());v['future_extension']={'opaque':['retained',3]};state.write_text(json.dumps(v))
    compare(h,root,'record_event',{'run_id':run,'kind':'exception','summary':'Synthetic compatibility event','evidence':['synthetic:test'],'idempotency_key':'parity:event'});count+=1
    assert json.loads(state.read_text())['future_extension']==v['future_extension']
    # Both implementations agree on exact generated view bytes.
    h.call('rebuild_pipeline',{'idempotency_key':'parity:python-view'})
    md_before=(root/'applications/pipeline.md').read_bytes();json_before=(root/'applications/pipeline.json').read_bytes()
    native(root,'rebuild_pipeline',{'idempotency_key':'parity:rebuild'})
    assert (root/'applications/pipeline.md').read_bytes()==md_before,'Markdown pipeline differs'
    assert (root/'applications/pipeline.json').read_bytes()==json_before,'JSON pipeline differs'
    # Transfer format works in both directions; source authority bytes are unchanged.
    old_bundle=Path(directory)/'python.jhb';export_bundle(root,old_bundle)
    restored=Path(directory)/'rust';command(restored,'workspace','import','--input',old_bundle)
    assert (restored/'applications/operations/state.json').read_bytes()==state.read_bytes()
    new_bundle=Path(directory)/'rust.jhb';command(restored,'workspace','export','--output',new_bundle)
    final=Path(directory)/'python-restored';import_bundle(final,new_bundle)
    original=json.loads(state.read_text());roundtrip=json.loads((final/'applications/operations/state.json').read_text())
    assert roundtrip['runs']==original['runs'] and roundtrip['attempts']==original['attempts']
    assert roundtrip['future_extension']==original['future_extension']
    assert roundtrip['events'][:len(original['events'])]==original['events']
    assert all(event['tool']=='rebuild_pipeline' for event in roundtrip['events'][len(original['events']):])
    assert len(build(final)[0])==1;count+=2
    # Python flock and native reader exclude one another on the shared lock.
    with (root/'applications/operations/.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        process=subprocess.Popen([str(BIN),'--root',str(root),'call','read_record'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        process.stdin.write('{"path":"applications/approved-answers.md"}');process.stdin.close()
        time.sleep(.15);assert process.poll() is None,'Rust ignored historical lock'
        fcntl.flock(lock,fcntl.LOCK_UN)
        output=process.stdout.read();process.wait(timeout=10);assert json.loads(output)['ok'];count+=1
    print(f'PASS cross-language parity: {count} traces, Python packet replay, unknown guard, journal fields, pipeline bytes, transfers both directions and cross-process lock')
