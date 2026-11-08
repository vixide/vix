import sys, json, re

log = open(sys.argv[1], 'a')
# The workspace root as a file:// URI (built by Vix, so it is a valid URI on
# Windows too, T772).
ROOT = sys.argv[2]

def read_msg():
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        line = line.decode('ascii').strip()
        if line == '':
            break
        k, _, v = line.partition(':')
        headers[k.strip().lower()] = v.strip()
    n = int(headers.get('content-length', '0'))
    return json.loads(sys.stdin.buffer.read(n))

def send(obj):
    data = json.dumps(obj).encode('utf-8')
    sys.stdout.buffer.write(b'Content-Length: %d\r\n\r\n' % len(data))
    sys.stdout.buffer.write(data)
    sys.stdout.buffer.flush()

def note(text):
    log.write(text + '\n'); log.flush()

while True:
    msg = read_msg()
    if msg is None:
        break
    method = msg.get('method') or ''
    mid = msg.get('id')
    op = re.match(r'workspace/(will|did)(Create|Rename|Delete)Files$', method)
    if method == 'initialize':
        filt = {'filters': [
            {'pattern': {'glob': '**/*.rs', 'matches': 'file'}},
            {'pattern': {'glob': '**/pkg*', 'matches': 'folder'}}]}
        ops = {}
        for k in ['willCreate', 'didCreate', 'willRename', 'didRename', 'willDelete', 'didDelete']:
            ops[k] = filt
        send({'jsonrpc':'2.0','id':mid,'result':{'capabilities':{
            'workspace': {'fileOperations': ops}}}})
        send({'jsonrpc':'2.0','id':900,'method':'client/registerCapability','params':{
            'registrations':[{'id':'w1','method':'workspace/didChangeWatchedFiles',
                'registerOptions':{'watchers':[{'globPattern':'**/Cargo.toml'},
                                               {'globPattern':'**/*.cfg'}]}}]}})
    elif method == 'textDocument/didOpen':
        uri = msg['params']['textDocument']['uri']
        send({'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{
            'uri': uri, 'diagnostics': [{'range':{'start':{'line':0,'character':0},
                'end':{'line':0,'character':1}},'severity':1,'message':'ready'}]}})
    elif op:
        for f in msg['params']['files']:
            names = f.get('uri') or (f['oldUri'] + ' ' + f['newUri'])
            note('%s %s %s' % (op.group(1), op.group(2), names))
        if op.group(1) == 'will':
            send({'jsonrpc':'2.0','id':mid,'result':{'changes':{
                ROOT + '/imports.rs':[{'range':{'start':{'line':0,'character':0},
                    'end':{'line':0,'character':3}},'newText':'use'}]}}})
    elif method == 'workspace/didChangeWatchedFiles':
        for c in msg['params']['changes']:
            note('watch %d %s' % (c['type'], c['uri']))
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
