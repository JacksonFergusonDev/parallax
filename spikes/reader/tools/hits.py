import http.server,sys,base64
png=base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGMAAQAABQABDQottAAAAABJRU5ErkJggg==')
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(s):
        open(sys.argv[1],'a').write(s.path+'\n')
        s.send_response(200);s.send_header('Content-Type','image/png' if 'png' in s.path or 'img' in s.path else 'text/html');s.send_header('Content-Length',str(len(png)));s.end_headers();s.wfile.write(png)
    do_POST=do_GET
    def log_message(*a):pass
http.server.ThreadingHTTPServer(('127.0.0.1',8931),H).serve_forever()
