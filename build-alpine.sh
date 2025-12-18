#!/bin/sh

# InfraSim Alpine Build Script for Kubernetes Runner

echo "Starting Alpine build pipeline..."

# 1. Run Terraform plan
echo "Running Terraform plan..."
terraform plan -out=plan.tfplan

# 2. Apply infrastructure
echo "Applying Terraform configuration..."
terraform apply plan.tfplan

# 3. Wait for VM to be ready
echo "Waiting for VM..."
sleep 30

# 4. Get VM ID from Terraform output
VM_ID=$(terraform output -raw vm_id)

# 5. Start VM
echo "Starting VM $VM_ID..."
curl -X POST http://$DAEMON_ADDR/api/vms/$VM_ID/start

# 6. Wait for boot
echo "Waiting for VM to boot..."
sleep 60

# 7. Create memory snapshot
echo "Creating memory snapshot..."
SNAPSHOT_ID=$(curl -X POST http://$DAEMON_ADDR/api/vms/$VM_ID/snapshot \
  -H "Content-Type: application/json" \
  -d '{"name": "alpine-built", "include_memory": true}' | jq -r .snapshot_id)

echo "Build complete! Snapshot ID: $SNAPSHOT_ID"

# 8. Export snapshot as artifact
echo "Exporting snapshot..."
curl -X GET http://$DAEMON_ADDR/api/snapshots/$SNAPSHOT_ID > alpine-snapshot.qcow2

echo "Alpine memory snapshot ready: alpine-snapshot.qcow2"