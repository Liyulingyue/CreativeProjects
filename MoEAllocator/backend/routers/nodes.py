from fastapi import APIRouter, HTTPException
from pydantic import BaseModel
from typing import Optional

from backend.process_manager import manager
from backend.proxy import proxy

router = APIRouter(prefix="/api/nodes", tags=["nodes"])


class StartMasterRequest(BaseModel):
    node_id: str
    manifest_path: str
    http_port: Optional[int] = None
    expert_ids: Optional[str] = "0,1,2"


class StartWorkerRequest(BaseModel):
    node_id: str
    http_port: Optional[int] = None
    tcp_port: Optional[int] = None
    experts_dir: Optional[str] = None
    expert_ids: Optional[str] = None
    master_node_id: Optional[str] = None


class RegisterWorkerRequest(BaseModel):
    worker_node_id: str
    master_node_id: str


@router.get("")
async def list_nodes():
    return manager.list_nodes()


@router.post("/master")
async def create_master(req: StartMasterRequest):
    try:
        result = manager.start_master(
            node_id=req.node_id,
            manifest_path=req.manifest_path,
            http_port=req.http_port,
            expert_ids=req.expert_ids,
        )
        return result
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.post("/worker")
async def create_worker(req: StartWorkerRequest):
    try:
        master_url = None
        if req.master_node_id:
            master_url = manager.get_master_url(req.master_node_id)
            if not master_url:
                raise HTTPException(status_code=400, detail=f"Master '{req.master_node_id}' not found")

        result = manager.start_worker(
            node_id=req.node_id,
            http_port=req.http_port,
            tcp_port=req.tcp_port,
            experts_dir=req.experts_dir,
            expert_ids=req.expert_ids,
            master_url=master_url,
        )
        return result
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except HTTPException:
        raise
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
    expert_id: int
    layer_id: int


class UnloadExpertRequest(BaseModel):
    node_id: str
    expert_id: int
    layer_id: int


@router.post("/master/load_expert")
async def master_load_expert(req: LoadExpertRequest):
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


@router.post("/master/unload_expert")
async def master_unload_expert(req: UnloadExpertRequest):
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
