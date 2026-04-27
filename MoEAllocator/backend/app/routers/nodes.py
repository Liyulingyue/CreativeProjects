from fastapi import APIRouter, HTTPException
from pydantic import BaseModel
from typing import Optional

from app.process_manager import manager
from app.proxy import proxy

router = APIRouter(prefix="/api/nodes", tags=["nodes"])


class StartMasterRequest(BaseModel):
    node_id: str
    manifest_path: str
    http_port: Optional[int] = None
    host: Optional[str] = '127.0.0.1'
    expert_ids: Optional[str] = None
    python_env: Optional[str] = 'venv'
    custom_python: Optional[str] = None


class StartWorkerRequest(BaseModel):
    node_id: str
    http_port: Optional[int] = None
    tcp_port: Optional[int] = None
    host: Optional[str] = '127.0.0.1'
    experts_dir: Optional[str] = None
    expert_ids: Optional[list[int]] = None
    master_node_id: Optional[str] = None
    master_url: Optional[str] = None
    python_env: Optional[str] = 'venv'
    custom_python: Optional[str] = None


class RegisterWorkerRequest(BaseModel):
    worker_node_id: str
    master_node_id: str


class AddRemoteNodeRequest(BaseModel):
    node_id: str
    node_type: str
    base_url: str
    tcp_port: Optional[int] = None


@router.get("")
async def list_nodes():
    result = manager.list_nodes()
    master_ids = [n for n in result if n['node_type'] == 'master']

    for master in master_ids:
        try:
            base_url = manager.get_node_url(master['node_id'])
            status_code, data = proxy.get(base_url, "/workers")
            if status_code == 200 and isinstance(data, dict) and 'workers' in data:
                for wid, winfo in data['workers'].items():
                    result.append({
                        'node_id': wid,
                        'node_type': 'worker',
                        'http_port': winfo.get('http_port'),
                        'tcp_port': winfo.get('tcp_port'),
                        'pid': None,
                        'alive': True,
                        'log_file': None,
                        'is_remote': True,
                        'master_id': master['node_id'],
                    })
        except Exception:
            pass

    return result


@router.get("/detect-python")
async def detect_python():
    return manager.get_detected_envs()


@router.post("/master")
async def create_master(req: StartMasterRequest):
    try:
        result = manager.start_master(
            node_id=req.node_id,
            manifest_path=req.manifest_path,
            http_port=req.http_port,
            host=req.host or '127.0.0.1',
            expert_ids=req.expert_ids,
            python_env=req.python_env or 'venv',
            custom_python=req.custom_python,
        )
        return result
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.post("/worker")
async def create_worker(req: StartWorkerRequest):
    try:
        master_url = req.master_url or None
        if not master_url and req.master_node_id:
            master_url = manager.get_master_url(req.master_node_id)
            if not master_url:
                raise HTTPException(status_code=400, detail=f"Master '{req.master_node_id}' not found")

        result = manager.start_worker(
            node_id=req.node_id,
            http_port=req.http_port,
            tcp_port=req.tcp_port,
            host=req.host or '127.0.0.1',
            experts_dir=req.experts_dir,
            expert_ids=req.expert_ids,
            master_url=master_url,
            python_env=req.python_env or 'venv',
            custom_python=req.custom_python,
        )
        return result
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.post("/remote")
async def add_remote_node(req: AddRemoteNodeRequest):
    try:
        return manager.add_remote_node(
            node_id=req.node_id,
            node_type=req.node_type,
            base_url=req.base_url,
            tcp_port=req.tcp_port,
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.delete("/{node_id}")
async def delete_node(node_id: str):
    try:
        return manager.stop_node(node_id)
    except ValueError as e:
        raise HTTPException(status_code=404, detail=str(e))


@router.get("/{node_id}")
async def get_node(node_id: str):
    info = manager.get_node(node_id)
    if info is None:
        raise HTTPException(status_code=404, detail=f"Node '{node_id}' not found")
    return info


@router.get("/{node_id}/status")
async def get_node_status(node_id: str):
    info = manager.get_node(node_id)
    if info is None:
        master_nodes = [n for n in manager.list_nodes() if n['node_type'] == 'master']
        for master in master_nodes:
            try:
                master_url = manager.get_node_url(master['node_id'])
                mw_status, mw_data = proxy.get(master_url, "/workers")
                if mw_status == 200 and isinstance(mw_data, dict) and 'workers' in mw_data:
                    for wid, winfo in mw_data['workers'].items():
                        if wid == node_id:
                            manager._remote_nodes[node_id] = {
                                'node_id': node_id,
                                'node_type': 'worker',
                                'http_port': winfo.get('http_port'),
                                'tcp_port': winfo.get('tcp_port'),
                                'host': winfo.get('host', '127.0.0.1'),
                            }
                            base_url = f"http://{winfo.get('host', '127.0.0.1')}:{winfo.get('http_port')}"
                            status_code, data = proxy.get(base_url, "/status")
                            if status_code != 200:
                                raise HTTPException(status_code=status_code, detail=data)
                            return data
            except HTTPException:
                raise
            except Exception:
                pass
        raise HTTPException(status_code=404, detail=f"Node '{node_id}' not found")

    base_url = manager.get_node_url(node_id)
    status_code, data = proxy.get(base_url, "/status")

    if status_code != 200:
        raise HTTPException(status_code=status_code, detail=data if isinstance(data, str) else data.get('error', 'Unknown error'))

    return data


@router.get("/{node_id}/workers")
async def get_master_workers(node_id: str):
    info = manager.get_node(node_id)
    if info is None:
        raise HTTPException(status_code=404, detail=f"Node '{node_id}' not found")

    base_url = manager.get_node_url(node_id)
    status_code, data = proxy.get(base_url, "/workers")

    if status_code != 200:
        raise HTTPException(status_code=status_code, detail=data if isinstance(data, str) else data.get('error', 'Unknown error'))

    return data


@router.post("/{master_id}/workers")
async def register_worker_to_master(master_id: str, req: RegisterWorkerRequest):
    master_info = manager.get_node(master_id)
    if master_info is None:
        raise HTTPException(status_code=404, detail=f"Master '{master_id}' not found")

    worker_info = manager.get_node(req.worker_node_id)
    if worker_info is None:
        raise HTTPException(status_code=404, detail=f"Worker '{req.worker_node_id}' not found")

    master_url = manager.get_master_url(master_id)
    body = {
        'worker_id': req.worker_node_id,
        'http_port': worker_info['http_port'],
        'tcp_port': worker_info['tcp_port'],
    }

    status_code, data = proxy.post(master_url, "/workers", json_data=body)

    if status_code != 200:
        raise HTTPException(status_code=status_code, detail=data if isinstance(data, str) else data.get('error', 'Unknown error'))

    return data


class LoadExpertRequest(BaseModel):
    node_id: str
    expert_id: Optional[int] = None
    layer_id: Optional[int] = None


class BatchLoadExpertRequest(BaseModel):
    node_id: str
    layer_id: int
    expert_ids: list[int] = []


class BatchUnloadExpertRequest(BaseModel):
    node_id: str
    layer_id: int
    expert_ids: list[int] = []


class UnloadExpertRequest(BaseModel):
    node_id: str
    expert_id: Optional[int] = None
    layer_id: Optional[int] = None


@router.post("/master/load_expert")
async def master_load_expert(req: LoadExpertRequest):
    if req.expert_id is None or req.layer_id is None:
        raise HTTPException(status_code=400, detail=f"expert_id and layer_id are required")
    info = manager.get_node(req.node_id)
    if info is None:
        raise HTTPException(status_code=404, detail=f"Node '{req.node_id}' not found")

    base_url = manager.get_node_url(req.node_id)
    body = {
        'expert_id': req.expert_id,
        'layer_id': req.layer_id,
    }
    status_code, data = proxy.post(base_url, "/load_expert", json_data=body)

    if status_code != 200:
        raise HTTPException(status_code=status_code, detail=data if isinstance(data, str) else data.get('error', 'Unknown error'))

    return data


@router.post("/master/load_experts")
async def master_load_experts(req: BatchLoadExpertRequest):
    info = manager.get_node(req.node_id)
    if info is None:
        raise HTTPException(status_code=404, detail=f"Node '{req.node_id}' not found")

    base_url = manager.get_node_url(req.node_id)
    results = []
    for eid in req.expert_ids:
        body = {'expert_id': eid, 'layer_id': req.layer_id}
        status_code, data = proxy.post(base_url, "/load_expert", json_data=body)
        results.append({'expert_id': eid, 'layer_id': req.layer_id, 'status': status_code, 'data': data if status_code == 200 else None})

    return {'results': results}


@router.post("/master/unload_expert")
async def master_unload_expert(req: UnloadExpertRequest):
    if req.expert_id is None or req.layer_id is None:
        raise HTTPException(status_code=400, detail=f"expert_id and layer_id are required")
    info = manager.get_node(req.node_id)
    if info is None:
        raise HTTPException(status_code=404, detail=f"Node '{req.node_id}' not found")

    base_url = manager.get_node_url(req.node_id)
    body = {
        'expert_id': req.expert_id,
        'layer_id': req.layer_id,
    }
    status_code, data = proxy.post(base_url, "/unload_expert", json_data=body)

    if status_code != 200:
        raise HTTPException(status_code=status_code, detail=data if isinstance(data, str) else data.get('error', 'Unknown error'))

    return data


@router.post("/master/unload_experts")
async def master_unload_experts(req: BatchUnloadExpertRequest):
    info = manager.get_node(req.node_id)
    if info is None:
        raise HTTPException(status_code=404, detail=f"Node '{req.node_id}' not found")

    base_url = manager.get_node_url(req.node_id)
    results = []
    for eid in req.expert_ids:
        body = {'expert_id': eid, 'layer_id': req.layer_id}
        status_code, data = proxy.post(base_url, "/unload_expert", json_data=body)
        results.append({'expert_id': eid, 'layer_id': req.layer_id, 'status': status_code})

    return {'results': results}


class LoadExpertToWorkerRequest(BaseModel):
    node_id: str
    worker_id: str
    expert_id: int
    layer_id: int
    file_path: Optional[str] = None


class UnloadExpertFromWorkerRequest(BaseModel):
    node_id: str
    worker_id: str
    expert_id: int
    layer_id: int


@router.post("/master/load_expert_to_worker")
async def master_load_expert_to_worker(req: LoadExpertToWorkerRequest):
    info = manager.get_node(req.node_id)
    if info is None:
        raise HTTPException(status_code=404, detail=f"Node '{req.node_id}' not found")

    base_url = manager.get_node_url(req.node_id)
    body = {
        'worker_id': req.worker_id,
        'expert_id': req.expert_id,
        'layer_id': req.layer_id,
    }
    if req.file_path:
        body['file_path'] = req.file_path

    status_code, data = proxy.post(base_url, "/load_expert_to_worker", json_data=body)

    if status_code != 200:
        raise HTTPException(status_code=status_code, detail=data if isinstance(data, str) else data.get('error', 'Unknown error'))

    return data


@router.post("/master/unload_expert_from_worker")
async def master_unload_expert_from_worker(req: UnloadExpertFromWorkerRequest):
    info = manager.get_node(req.node_id)
    if info is None:
        raise HTTPException(status_code=404, detail=f"Node '{req.node_id}' not found")

    base_url = manager.get_node_url(req.node_id)
    body = {
        'worker_id': req.worker_id,
        'expert_id': req.expert_id,
        'layer_id': req.layer_id,
    }

    status_code, data = proxy.post(base_url, "/unload_expert_from_worker", json_data=body)

    if status_code != 200:
        raise HTTPException(status_code=status_code, detail=data if isinstance(data, str) else data.get('error', 'Unknown error'))

    return data


class WorkerConnectRequest(BaseModel):
    master_url: str


@router.post("/{node_id}/connect")
async def worker_connect(node_id: str, req: WorkerConnectRequest):
    info = manager.get_node(node_id)
    if info is None:
        raise HTTPException(status_code=404, detail=f"Node '{node_id}' not found")

    base_url = manager.get_node_url(node_id)
    status_code, data = proxy.post(base_url, "/register", json_data={
        "master_url": req.master_url,
    })

    if status_code != 200:
        raise HTTPException(status_code=status_code, detail=data if isinstance(data, str) else data.get('error', 'Unknown error'))

    return data


@router.get("/{node_id}/logs")
async def get_node_logs(node_id: str):
    import os
    info = manager.get_node(node_id)
    if info is None:
        raise HTTPException(status_code=404, detail=f"Node '{node_id}' not found")

    log_file = info.get('log_file')
    if not log_file or not os.path.exists(log_file):
        return {'content': '(无可用日志)'}

    with open(log_file, 'r') as f:
        lines = f.readlines()
    return {'content': ''.join(lines[-500:])}
