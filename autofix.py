import subprocess
import json

def run_cargo():
    result = subprocess.run(['cargo', 'check', '--message-format=json'], capture_output=True, text=True)
    errors = []
    for line in result.stdout.split('\n'):
        if not line: continue
        try:
            msg = json.loads(line)
            if msg.get('reason') == 'compiler-message' and msg['message']['code'] and msg['message']['code']['code'] == 'E0004':
                span = msg['message']['spans'][0]
                errors.append(span)
        except:
            pass
    return errors

while True:
    errs = run_cargo()
    if not errs: break
    # Fix the first error
    span = errs[0]
    file = span['file_name']
    line_end = span['line_end'] # The match block starts here, wait, we need the end of the match block!
    # The help message usually tells us where to insert
