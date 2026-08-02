import urllib.request
import ssl
import tarfile
import io
import re

ctx = ssl.create_default_context()
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE

url = 'https://registry.npmjs.org/@mediapipe/tasks-vision/-/tasks-vision-1.0.0.tgz'
print('Downloading...')
response = urllib.request.urlopen(url, timeout=30, context=ctx)
data = response.read()

print('Extracting and searching...')
with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as tar:
    for member in tar.getmembers():
        if member.isfile():
            name = member.name.lower()
            if 'vision_bundle' in name or 'model' in name or '.task' in name:
                print(f'  {member.name}')

            if member.size < 500000 and (name.endswith('.js') or name.endswith('.ts') or name.endswith('.json')):
                try:
                    f = tar.extractfile(member)
                    if f:
                        content = f.read().decode('utf-8', errors='ignore')
                        urls = re.findall(r'https://storage\.googleapis\.com/[^\s"\']+', content)
                        if urls:
                            print(f'\n{member.name}:')
                            for u in urls:
                                print(f'  {u}')
                except:
                    pass

print('\nDone')