import http.server
import socketserver

PORT = 3001

class CleanDashboardHandler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path in ['/', '/dashboard', '/dashboard/']:
            with open('dashboard.html', 'r', encoding='utf-8') as f:
                content = f.read()
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.end_headers()
            self.wfile.write(content.encode('utf-8'))
        elif self.path == '/health':
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(b'{"status":"ok","service":"vivekarouter"}')
        else:
            self.send_response(404)
            self.end_headers()

if __name__ == '__main__':
    with socketserver.TCPServer(('127.0.0.1', PORT), CleanDashboardHandler) as httpd:
        print(f"VivekaRouter Dashboard running at http://localhost:{PORT}")
        httpd.serve_forever()
