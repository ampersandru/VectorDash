import unittest
from unittest.mock import patch
import ipaddress
from mcp_network import connection_details,lan_ip

class NetworkTests(unittest.TestCase):
    def test_copy_auth_off(self):
        self.assertNotIn('headers',connection_details({'mcp_auth':False,'mcp_token':'old'}))
    def test_copy_lan(self):
        with patch('mcp_network.lan_ip',return_value='192.168.1.42'):
            value=connection_details({'mcp_lan':True,'mcp_port':8791,'mcp_token':'token'})
        self.assertEqual(value['url'],'http://192.168.1.42:8791/mcp')
        self.assertEqual(value['headers']['Authorization'],'Bearer token')
    def test_no_interface(self):
        with patch('mcp_network.lan_interfaces',return_value=[]):
            with self.assertRaises(ValueError):lan_ip()

    def test_lan_host_and_origin_guards(self):
        import json
        import urllib.request
        import urllib.error
        from mcp_server import McpServer
        config={'mcp_lan':True,'mcp_auth':False,'mcp_port':0}
        with patch('mcp_server.lan_interfaces',return_value=[('192.168.1.42',ipaddress.ip_network('192.168.1.0/24'))]),patch('mcp_server.lan_ip',return_value='192.168.1.42'):
            server=McpServer(None,lambda:config);server.start()
        self.addCleanup(server.stop)
        def request(host,origin=None):
            headers={'Content-Type':'application/json','Host':host}
            if origin:headers['Origin']=origin
            data=json.dumps({'jsonrpc':'2.0','id':1,'method':'ping'}).encode()
            with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{server.port}/mcp',data,headers),timeout=3) as response:
                return json.load(response)
        host=f'192.168.1.42:{server.port}'
        self.assertEqual(server.url,f'http://{host}/mcp')
        self.assertEqual(request(host)['result'],{})
        for bad_host,origin in [('evil.example',None),(host,'https://evil.example')]:
            with self.assertRaises(urllib.error.HTTPError) as error:request(bad_host,origin)
            self.assertEqual(error.exception.code,403);error.exception.close()
