# Fake org.freedesktop.portal.Desktop for pick_test, on a private bus. Like xdg-desktop-portal 1.18 it sends
# Request.Response only to the caller and drops the request when the caller disconnects. Logs OpenFile's
# arguments and Request.Close to $PICK_FAKE_LOG. Title "cancel": code 1; "hang": no answer; else picks
# file:///home/u/My%20Project.
import os
from gi.repository import Gio, GLib

XML = '''<node>
<interface name='org.freedesktop.portal.FileChooser'>
  <method name='OpenFile'><arg type='s' direction='in'/><arg type='s' direction='in'/>
    <arg type='a{sv}' direction='in'/><arg type='o' direction='out'/></method>
</interface>
<interface name='org.freedesktop.portal.Request'><method name='Close'/></interface>
</node>'''
node = Gio.DBusNodeInfo.new_for_xml(XML)
log = os.environ['PICK_FAKE_LOG']
reqs = {}

def write(s):
    with open(log, 'a') as f:
        f.write(s + '\n')

def request_call(conn, sender, path, iface, method, params, inv):
    write('closed ' + path)
    inv.return_value(None)

def chooser_call(conn, sender, path, iface, method, params, inv):
    parent, title, opts = params.unpack()
    write(params.print_(True))
    h = '/org/freedesktop/portal/desktop/request/%s/%s' % (sender[1:].replace('.', '_'), opts['handle_token'])
    reqs[h] = conn.register_object(h, node.interfaces[1], request_call, None, None)
    inv.return_value(GLib.Variant('(o)', (h,)))
    if title == 'hang':
        return
    code = 1 if title == 'cancel' else 0
    res = {} if code else {'uris': GLib.Variant('as', ['file:///home/u/My%20Project'])}
    def answer():
        if h in reqs:
            conn.emit_signal(sender, h, 'org.freedesktop.portal.Request', 'Response',
                GLib.Variant('(ua{sv})', (code, res)))
        return False
    GLib.timeout_add(100, answer)

def gone(conn, snd, path, i, sig, p, u):
    name, old, new = p.unpack()
    if not new:
        for h in [h for h in reqs if h.split('/')[-2] == name[1:].replace('.', '_')]:
            conn.unregister_object(reqs.pop(h))

def acquired(conn, name):
    conn.register_object('/org/freedesktop/portal/desktop', node.interfaces[0], chooser_call, None, None)
    conn.signal_subscribe('org.freedesktop.DBus', 'org.freedesktop.DBus', 'NameOwnerChanged', None, None,
        Gio.DBusSignalFlags.NONE, gone, None)

Gio.bus_own_name(Gio.BusType.SESSION, 'org.freedesktop.portal.Desktop', Gio.BusNameOwnerFlags.NONE,
    acquired, lambda c, n: write('ready'), lambda c, n: os._exit(1))
GLib.MainLoop().run()
