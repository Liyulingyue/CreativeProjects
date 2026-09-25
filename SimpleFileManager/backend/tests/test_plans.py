"""Tests for the plans router (create, approve, reject, execute, delete)."""
from tests.conftest import client, get_storage_root, create_test_file


class TestCreatePlan:
    def test_create_plan(self):
        res = client.post("/api/plans", json={
            "title": "Test Plan",
            "summary": "A test plan",
            "source": "manual",
            "actions": [
                {"action_type": "create_folder", "target_path": "new_folder", "reason": "test"},
            ],
        })
        assert res.status_code == 200
        data = res.json()
        assert data["status"] == "pending"
        assert data["title"] == "Test Plan"
        assert len(data["actions"]) == 1

    def test_create_empty_plan_fails(self):
        res = client.post("/api/plans", json={
            "title": "Empty",
            "summary": "",
            "source": "manual",
            "actions": [],
        })
        assert res.status_code == 400


class TestApproveReject:
    def test_approve_then_execute(self):
        create_plan = client.post("/api/plans", json={
            "title": "Create folder plan",
            "summary": "",
            "source": "manual",
            "actions": [
                {"action_type": "create_folder", "target_path": "approved_dir", "reason": "test"},
            ],
        })
        plan_id = create_plan.json()["id"]

        approve = client.post(f"/api/plans/{plan_id}/approve")
        assert approve.json()["status"] == "approved"

        execute = client.post(f"/api/plans/{plan_id}/execute")
        assert execute.json()["status"] == "executed"
        assert (get_storage_root() / "approved_dir").is_dir()

    def test_reject_plan(self):
        create_plan = client.post("/api/plans", json={
            "title": "Rejected plan",
            "summary": "",
            "source": "manual",
            "actions": [
                {"action_type": "create_folder", "target_path": "rejected_dir", "reason": "test"},
            ],
        })
        plan_id = create_plan.json()["id"]

        reject = client.post(f"/api/plans/{plan_id}/reject")
        assert reject.json()["status"] == "rejected"
        assert not (get_storage_root() / "rejected_dir").exists()

    def test_execute_without_approval_fails(self):
        create_plan = client.post("/api/plans", json={
            "title": "No approval",
            "summary": "",
            "source": "manual",
            "actions": [
                {"action_type": "create_folder", "target_path": "no_approval", "reason": "test"},
            ],
        })
        plan_id = create_plan.json()["id"]
        execute = client.post(f"/api/plans/{plan_id}/execute")
        assert execute.status_code == 400


class TestExecuteActions:
    def test_move_action(self):
        create_test_file("move_me.txt", "content")
        client.post("/api/plans", json={
            "title": "Move plan",
            "summary": "",
            "source": "manual",
            "actions": [
                {"action_type": "move", "source_path": "move_me.txt", "target_path": "moved.txt", "reason": "test"},
            ],
        })

    def test_delete_action(self):
        create_test_file("delete_me.txt", "content")
        create_plan = client.post("/api/plans", json={
            "title": "Delete plan",
            "summary": "",
            "source": "manual",
            "actions": [
                {"action_type": "delete", "source_path": "delete_me.txt", "reason": "test"},
            ],
        })
        plan_id = create_plan.json()["id"]
        client.post(f"/api/plans/{plan_id}/approve")
        client.post(f"/api/plans/{plan_id}/execute")
        assert not (get_storage_root() / "delete_me.txt").exists()


class TestListPlans:
    def test_list_plans(self):
        client.post("/api/plans", json={
            "title": "Plan A",
            "summary": "",
            "source": "manual",
            "actions": [{"action_type": "create_folder", "target_path": "a", "reason": ""}],
        })
        client.post("/api/plans", json={
            "title": "Plan B",
            "summary": "",
            "source": "manual",
            "actions": [{"action_type": "create_folder", "target_path": "b", "reason": ""}],
        })
        res = client.get("/api/plans")
        assert res.status_code == 200
        data = res.json()
        assert len(data["plans"]) >= 2

    def test_filter_by_status(self):
        create_plan = client.post("/api/plans", json={
            "title": "Pending plan",
            "summary": "",
            "source": "manual",
            "actions": [{"action_type": "create_folder", "target_path": "p", "reason": ""}],
        })
        plan_id = create_plan.json()["id"]
        client.post(f"/api/plans/{plan_id}/approve")

        res = client.get("/api/plans", params={"status": "approved"})
        data = res.json()
        assert all(p["status"] == "approved" for p in data["plans"])


class TestDeletePlan:
    def test_delete_plan(self):
        create_plan = client.post("/api/plans", json={
            "title": "To delete",
            "summary": "",
            "source": "manual",
            "actions": [{"action_type": "create_folder", "target_path": "td", "reason": ""}],
        })
        plan_id = create_plan.json()["id"]
        res = client.delete(f"/api/plans/{plan_id}")
        assert res.status_code == 200
        assert client.get(f"/api/plans/{plan_id}").status_code == 404
