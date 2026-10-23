"""Verify real Qev/projector cleanup on normal shutdown and forced owner termination."""
import argparse,ctypes,json,os,pathlib,subprocess,time,uuid
from ctypes import wintypes as w
p=argparse.ArgumentParser();p.add_argument('--test-exe',required=True);p.add_argument('--runtime',required=True);p.add_argument('--model',required=True);p.add_argument('--mmproj',required=True);a=p.parse_args()
root=(pathlib.Path('windows/.smoke/decision-exit')/uuid.uuid4().hex).resolve();root.mkdir(parents=True)
k=ctypes.WinDLL('kernel32',use_last_error=True);k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE;k.WaitForSingleObject.argtypes=[w.HANDLE,w.DWORD];k.WaitForSingleObject.restype=w.DWORD;k.CloseHandle.argtypes=[w.HANDLE]
for mode in ['graceful','force']:
 report=root/(mode+'.json');log=(root/(mode+'.log')).open('wb')
 env={**os.environ,'VECTORDASH_QEV_TEST_RUNTIME':str(pathlib.Path(a.runtime).resolve()),'VECTORDASH_QEV_TEST_VISION_MODEL':str(pathlib.Path(a.model).resolve()),'VECTORDASH_QEV_TEST_MMPROJ':str(pathlib.Path(a.mmproj).resolve()),'VECTORDASH_QEV_EXIT_REPORT':str(report),'VECTORDASH_QEV_EXIT_MODE':mode}
 owner=subprocess.Popen([str(pathlib.Path(a.test_exe).resolve()),'decision::tests::qev_exit_owner_fixture','--exact','--ignored','--nocapture'],env=env,stdout=log,stderr=log,creationflags=subprocess.CREATE_NO_WINDOW)
 handle=None
 try:
  deadline=time.monotonic()+45
  while not report.exists() and time.monotonic()<deadline:
   assert owner.poll() is None,'Owner exited before model loaded';time.sleep(.05)
  data=json.loads(report.read_text());assert data['owner']==owner.pid
  handle=k.OpenProcess(0x100000|0x1000,False,data['model'])
  if mode=='force':
   assert handle and k.WaitForSingleObject(handle,0)==258,'Model must be alive before owner is terminated'
   owner.terminate()
  owner.wait(timeout=15)
  if mode=='graceful':assert owner.returncode==0
  if handle:assert k.WaitForSingleObject(handle,5000)==0,'Owned Qev process survived its owner'
  else:assert ctypes.get_last_error()==87,'Unexpected failure querying owned model process'
  print('PASS',mode,'exit terminated real Qev and its vision projector',flush=True)
 finally:
  if owner.poll() is None:owner.terminate();owner.wait(timeout=10)
  if handle:k.CloseHandle(handle)
  log.close()
print('Artifacts:',root,flush=True)
