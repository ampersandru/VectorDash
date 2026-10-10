"""Local network discovery and connection details, without sending network traffic."""
import ipaddress
import socket
import psutil


def lan_interfaces():
    interfaces=[]
    stats=psutil.net_if_stats()
    for name,addresses in psutil.net_if_addrs().items():
        if name not in stats or not stats[name].isup:
            continue
        for address in addresses:
            if address.family!=socket.AF_INET or not address.netmask:
                continue
            ip=ipaddress.ip_address(address.address)
            if ip.is_private and not ip.is_loopback and not ip.is_link_local:
                interfaces.append((str(ip),ipaddress.ip_network(f'{ip}/{address.netmask}',strict=False)))
    return interfaces


def lan_ip():
    addresses=[address for address,_ in lan_interfaces()]
    if not addresses:
        raise ValueError('No active local-network IPv4 address found. Connect to Wi-Fi or Ethernet first.')
    try:
        with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as probe:
            # UDP connect only selects a route; no data is sent.
            probe.connect(('192.0.2.1',9))
            preferred=probe.getsockname()[0]
        if preferred in addresses:
            return preferred
    except OSError:
        pass
    return addresses[0]


def connection_details(settings):
    host=lan_ip() if settings.get('mcp_lan',False) else '127.0.0.1'
    config={'url':f'http://{host}:{settings.get("mcp_port",8790)}/mcp','transport':'streamable-http'}
    if settings.get('mcp_auth',True):
        config['headers']={'Authorization':'Bearer '+settings.get('mcp_token','')}
    return config
