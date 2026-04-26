import httpx


class ProxyClient:
    def __init__(self):
        self.client = httpx.Client(timeout=30.0, follow_redirects=True, proxy=None, trust_env=False)

    def _url(self, base_url: str, path: str) -> str:
        return f"{base_url.rstrip('/')}/{path.lstrip('/')}"

    def get(self, base_url: str, path: str = "/"):
        try:
            resp = self.client.get(self._url(base_url, path))
            return resp.status_code, resp.json() if resp.headers.get('content-type', '').startswith('application/json') else resp.text
        except Exception as e:
            return 500, {'error': str(e)}

    def post(self, base_url: str, path: str, json_data=None):
        try:
            resp = self.client.post(self._url(base_url, path), json=json_data)
            return resp.status_code, resp.json() if resp.headers.get('content-type', '').startswith('application/json') else resp.text
        except Exception as e:
            return 500, {'error': str(e)}

    def delete(self, base_url: str, path: str = "/"):
        try:
            resp = self.client.delete(self._url(base_url, path))
            return resp.status_code, resp.json() if resp.headers.get('content-type', '').startswith('application/json') else resp.text
        except Exception as e:
            return 500, {'error': str(e)}


proxy = ProxyClient()
