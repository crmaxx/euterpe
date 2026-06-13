//! Classic NDR (DCE/RPC transfer syntax v2) srvsvc NetrShareEnum for Samba and similar servers.
//!
//! The `smb` 0.11.2 client binds only NDR64; Samba rejects that with
//! `ProposedTransferSyntaxesNotSupported`. Windows typically accepts NDR64.
//!
//! Samba accepts the enum request over SMB2 pipe **Write/Read** (see docs/dumps/app_smb.txt).
//! FSCTL transceive for the same stub returns `nca_s_fault_ndr` (see docs/dumps/app5_smb.txt).

use std::io::{Cursor, Read};

use smb::resource::{Pipe, PipeRpcConnection};
use smb::{Error as SmbError, Result as SmbResult};
use smb_rpc::SmbRpcError;
use smb_rpc::pdu::*;

const SRVSVC_OPNUM_NET_SHARE_ENUM_ALL: u16 = 0x0f;
const PACKED_DREP: u32 = 0x10;

const SRVSVC_SYNTAX: DceRpcSyntaxId = DceRpcSyntaxId {
    uuid: smb_dtyp::make_guid!("4b324fc8-1670-01d3-1278-5a47bf6ee188"),
    version: 3,
};

const NDR_TRANSFER_SYNTAX: DceRpcSyntaxId = DceRpcSyntaxId {
    uuid: smb_dtyp::make_guid!("8a885d04-1ceb-11c9-9fe8-08002b104860"),
    version: 2,
};

/// Samba/macOS-style NDR referent ids.
const NDR_REF_BASE: u32 = 0x0002_0000;

pub(crate) fn rpc_server_name(host: &str) -> String {
    let host_only = host.split(':').next().unwrap_or(host);
    let stripped = host_only.trim_start_matches('\\');
    format!(r"\\{stripped}")
}

/// Returns true when the upstream `smb` crate failed to bind NDR64 for srvsvc.
pub(crate) fn is_ndr64_bind_rejection(err: &SmbError) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("bindack")
        && (msg.contains("proposedtransfersyntaxesnotsupported")
            || msg.contains("providerrejection")
            || msg.contains("71710533-beba-4937-8319-b5dbef9ccc36"))
}

struct NdrPipeRpc {
    pipe: Pipe,
    next_call_id: u32,
    context_id: u16,
}

pub(crate) async fn list_shares(pipe: Pipe, server_name: &str) -> SmbResult<Vec<String>> {
    let mut conn = NdrPipeRpc::connect(pipe).await?;
    let server = rpc_server_name(server_name);
    let stub = encode_netr_share_enum_in(&server);
    let response = conn
        .send_receive_raw(SRVSVC_OPNUM_NET_SHARE_ENUM_ALL, &stub)
        .await
        .map_err(map_rpc_error)?;
    decode_netr_share_enum_out(&response).map_err(SmbError::InvalidMessage)
}

impl NdrPipeRpc {
    async fn connect(mut pipe: Pipe) -> SmbResult<Self> {
        let transfer_syntaxes: [DceRpcSyntaxId; 1] = [NDR_TRANSFER_SYNTAX];
        let context_elements = Self::make_bind_contexts(SRVSVC_SYNTAX, &transfer_syntaxes);

        const START_CALL_ID: u32 = 2;
        const DEFAULT_FRAG_LIMIT: u16 = 4280;
        let bind_ack_pkt = PipeRpcConnection::dcerpc_pipe_exchange(
            &mut pipe,
            START_CALL_ID,
            DcRpcCoPktBind {
                max_xmit_frag: DEFAULT_FRAG_LIMIT,
                max_recv_frag: DEFAULT_FRAG_LIMIT,
                assoc_group_id: 0,
                context_elements,
            }
            .into(),
        )
        .await?;

        let bind_ack = match bind_ack_pkt.content() {
            DcRpcCoPktResponseContent::BindAck(bind_ack) => bind_ack,
            other => {
                return Err(SmbError::InvalidMessage(format!(
                    "Expected BindAck, got: {other:?}"
                )));
            }
        };

        let context_id = Self::check_bind_results(bind_ack, &transfer_syntaxes)?;

        Ok(Self {
            pipe,
            next_call_id: START_CALL_ID + 1,
            context_id,
        })
    }

    fn make_bind_contexts(
        syntax_id: DceRpcSyntaxId,
        transfer_syntaxes: &[DceRpcSyntaxId],
    ) -> Vec<DcRpcCoPktBindContextElement> {
        transfer_syntaxes
            .iter()
            .enumerate()
            .map(|(i, syntax)| DcRpcCoPktBindContextElement {
                context_id: i as u16,
                abstract_syntax: syntax_id.clone(),
                transfer_syntaxes: vec![syntax.clone()],
            })
            .collect()
    }

    fn check_bind_results(
        bind_ack: &DcRpcCoPktBindAck,
        transfer_syntaxes: &[DceRpcSyntaxId],
    ) -> SmbResult<u16> {
        if bind_ack.results.len() != transfer_syntaxes.len() {
            return Err(SmbError::InvalidMessage(format!(
                "BindAck results length {} does not match transfer syntaxes length {}",
                bind_ack.results.len(),
                transfer_syntaxes.len()
            )));
        }
        let Some((indx, (ack_context, syntax))) = bind_ack
            .results
            .iter()
            .zip(transfer_syntaxes)
            .enumerate()
            .next()
        else {
            return Err(SmbError::InvalidMessage(
                "No accepted context ID found in BindAck".to_string(),
            ));
        };
        if ack_context.result != DceRpcCoPktBindAckDefResult::Acceptance {
            return Err(SmbError::InvalidMessage(format!(
                "BindAck result for syntax {syntax} was not acceptance: {ack_context:?}"
            )));
        }
        if &ack_context.syntax != syntax {
            return Err(SmbError::InvalidMessage(format!(
                "BindAck abstract syntax {} does not match expected {}",
                ack_context.syntax, syntax
            )));
        }
        Ok(indx as u16)
    }

    /// Pipe byte-stream Write/Read (matches successful app_smb capture, not FSCTL).
    async fn send_receive_raw(
        &mut self,
        opnum: u16,
        stub_input: &[u8],
    ) -> Result<Vec<u8>, SmbRpcError> {
        let call_id = self.next_call_id;
        self.next_call_id += 1;

        let rpc_reply = PipeRpcConnection::dcerpc_pipe_exchange(
            &mut self.pipe,
            call_id,
            DcRpcCoPktRequest {
                alloc_hint: DcRpcCoPktRequest::ALLOC_HINT_NONE,
                context_id: self.context_id,
                opnum,
                stub_data: stub_input.to_vec(),
            }
            .into(),
        )
        .await
        .map_err(|e| SmbRpcError::SendReceiveError(e.to_string()))?;

        if rpc_reply.packed_drep() != PACKED_DREP {
            return Err(SmbRpcError::SendReceiveError(format!(
                "Unsupported packed DREP: {}",
                rpc_reply.packed_drep()
            )));
        }
        if !rpc_reply.pfc_flags().first_frag() || !rpc_reply.pfc_flags().last_frag() {
            return Err(SmbRpcError::SendReceiveError(
                "Expected first and last RPC fragment flags".to_string(),
            ));
        }

        let response = match rpc_reply.content() {
            DcRpcCoPktResponseContent::Response(response) => response,
            content => {
                return Err(SmbRpcError::SendReceiveError(format!(
                    "Expected RPC Response PDU, got: {content:?}"
                )));
            }
        };

        if response.context_id != self.context_id {
            return Err(SmbRpcError::SendReceiveError(format!(
                "Response context ID {} does not match expected {}",
                response.context_id, self.context_id
            )));
        }

        Ok(response.stub_data.clone())
    }
}

fn map_rpc_error(err: SmbRpcError) -> SmbError {
    SmbError::InvalidMessage(err.to_string())
}

fn pad4(len: usize) -> usize {
    (4 - (len % 4)) % 4
}

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn read_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32, String> {
    let mut b = [0u8; 4];
    cursor
        .read_exact(&mut b)
        .map_err(|e| format!("truncated u32: {e}"))?;
    Ok(u32::from_le_bytes(b))
}

fn read_u16(cursor: &mut Cursor<&[u8]>) -> Result<u16, String> {
    let mut b = [0u8; 2];
    cursor
        .read_exact(&mut b)
        .map_err(|e| format!("truncated u16: {e}"))?;
    Ok(u16::from_le_bytes(b))
}

fn align_cursor(cursor: &mut Cursor<&[u8]>) {
    let pos = cursor.position() as usize;
    let pad = pad4(pos);
    if pad > 0 {
        cursor.set_position(cursor.position() + pad as u64);
    }
}

/// Conformant varying UTF-16 string (NDR).
fn encode_conformant_varying_wchar(buf: &mut Vec<u8>, s: &str, include_null: bool) {
    let mut units: Vec<u16> = s.encode_utf16().collect();
    if include_null {
        units.push(0);
    }
    let byte_len = units.len() * 2;
    let count = units.len() as u32;
    write_u32(buf, count);
    write_u32(buf, 0);
    write_u32(buf, count);
    for unit in &units {
        buf.extend_from_slice(&unit.to_le_bytes());
    }
    buf.extend(std::iter::repeat_n(0u8, pad4(byte_len)));
}

/// `srvsvc_NetShareEnumAll` with inline `NetShareInfoCtr` (docs/dumps/app_smb.pcap, 80 bytes for UNC).
fn encode_netr_share_enum_in(server_name: &str) -> Vec<u8> {
    let mut stub = Vec::new();
    write_u32(&mut stub, NDR_REF_BASE);
    encode_conformant_varying_wchar(&mut stub, server_name, true);
    write_info_ctr_level1_empty(&mut stub);
    write_u32(&mut stub, u32::MAX);
    write_u32(&mut stub, 0);
    write_u32(&mut stub, 0);
    stub
}

fn write_info_ctr_level1_empty(stub: &mut Vec<u8>) {
    write_u32(stub, 1);
    // [switch_type(level)] union discriminant for NetShareCtr1 (see app_smb.pcap vs app6_smb.pcap).
    write_u32(stub, 1);
    write_u32(stub, NDR_REF_BASE);
    write_u32(stub, 0);
    write_u32(stub, 0);
}

fn read_conformant_varying_wchar(cursor: &mut Cursor<&[u8]>) -> Result<String, String> {
    let max_count = read_u32(cursor)?;
    let offset = read_u32(cursor)?;
    let actual_count = read_u32(cursor)?;
    if offset != 0 {
        return Err(format!("unsupported string offset {offset}"));
    }
    if actual_count == 0 {
        return Ok(String::new());
    }
    if actual_count > max_count {
        return Err("string actual_count > max_count".into());
    }
    let mut units = Vec::with_capacity(actual_count as usize);
    for _ in 0..actual_count {
        units.push(read_u16(cursor)?);
    }
    align_cursor(cursor);
    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    String::from_utf16(&units[..end]).map_err(|e| format!("invalid UTF-16: {e}"))
}

/// Samba `srvsvc_NetShareInfoCtr` response (see docs/dumps/app_smb.txt / osx_smb.txt).
fn decode_netr_share_enum_out(stub: &[u8]) -> Result<Vec<String>, String> {
    let mut cursor = Cursor::new(stub);
    let level = read_u32(&mut cursor)?;
    if level != 1 {
        return Err(format!("unsupported NetShareInfoCtr level {level}"));
    }
    let union_tag = read_u32(&mut cursor)?;
    if union_tag != 1 {
        return Err(format!("unsupported NetShareInfoCtr union tag {union_tag}"));
    }
    let ctr1_ref = read_u32(&mut cursor)?;
    let mut names = Vec::new();
    if ctr1_ref != 0 {
        let count = read_u32(&mut cursor)?;
        let array_ref = read_u32(&mut cursor)?;
        if array_ref != 0 {
            let max_count = read_u32(&mut cursor)?;
            let n = count.min(max_count);
            let mut entries = Vec::with_capacity(n as usize);
            for _ in 0..n {
                entries.push((
                    read_u32(&mut cursor)?,
                    read_u32(&mut cursor)?,
                    read_u32(&mut cursor)?,
                ));
            }
            for (name_ref, _share_type, comment_ref) in entries {
                if name_ref != 0 {
                    let name = read_conformant_varying_wchar(&mut cursor)?;
                    if !name.is_empty() && !name.ends_with('$') {
                        names.push(name);
                    }
                }
                if comment_ref != 0 {
                    let _comment = read_conformant_varying_wchar(&mut cursor)?;
                }
            }
        }
    }
    let total_ref = read_u32(&mut cursor)?;
    if total_ref != 0 {
        let _total = read_u32(&mut cursor)?;
    }
    let resume_ref = read_u32(&mut cursor)?;
    if resume_ref != 0 {
        let _resume = read_u32(&mut cursor)?;
    }
    let return_status = read_u32(&mut cursor)?;
    if return_status != 0 {
        return Err(format!(
            "NetrShareEnumAll returned status 0x{return_status:08x}"
        ));
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP_SMB_UNC_REQUEST_HEX: &str = concat!(
        "00000200100000000000000010000000",
        "5c005c003100390032002e003100360038002e0030002e003100320034000000",
        "0100000001000000000002000000000000000000ffffffff0000000000000000",
    );

    #[test]
    fn encode_netr_share_enum_in_matches_app_smb_unc_golden() {
        let stub = encode_netr_share_enum_in(r"\\192.168.0.124");
        assert_eq!(stub.len(), 80, "app_smb.pcap NetShareEnumAll request stub");
        let golden: Vec<u8> = (0..APP_SMB_UNC_REQUEST_HEX.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&APP_SMB_UNC_REQUEST_HEX[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(stub, golden);
    }

    #[test]
    fn rpc_server_name_strips_port_and_duplicate_slashes() {
        assert_eq!(rpc_server_name(r"\\192.168.1.10:1445"), r"\\192.168.1.10");
        assert_eq!(rpc_server_name(r"\\\\host"), r"\\host");
    }

    #[test]
    fn is_ndr64_bind_rejection_detects_samba_error() {
        let err = SmbError::InvalidMessage(
            "BindAck result for syntax (71710533-beba-4937-8319-b5dbef9ccc36/1) was not acceptance: ProviderRejection ProposedTransferSyntaxesNotSupported".into(),
        );
        assert!(is_ndr64_bind_rejection(&err));
    }

    #[test]
    fn decode_samba_net_share_enum_response() {
        let mut stub = Vec::new();
        write_u32(&mut stub, 1);
        write_u32(&mut stub, 1);
        write_u32(&mut stub, 0x0002_0008);
        write_u32(&mut stub, 2);
        write_u32(&mut stub, 0x0002_000c);
        write_u32(&mut stub, 2);
        write_u32(&mut stub, 0x0002_0010);
        write_u32(&mut stub, 0);
        write_u32(&mut stub, 0x0002_0014);
        write_u32(&mut stub, 0x0002_0018);
        write_u32(&mut stub, 0x8000_0003);
        write_u32(&mut stub, 0x0002_001c);
        encode_conformant_varying_wchar(&mut stub, "dietpi", true);
        encode_conformant_varying_wchar(&mut stub, "DietPi Share", true);
        encode_conformant_varying_wchar(&mut stub, "IPC$", true);
        encode_conformant_varying_wchar(&mut stub, "IPC Service (gravenas server)", true);
        write_u32(&mut stub, 0x0002_0020);
        write_u32(&mut stub, 2);
        write_u32(&mut stub, 0);
        write_u32(&mut stub, 0);
        write_u32(&mut stub, 0);

        let names = decode_netr_share_enum_out(&stub).expect("decode");
        assert_eq!(names, vec!["dietpi".to_string()]);
    }

    #[test]
    fn decode_app2_single_ipc_share_response() {
        let mut stub = Vec::new();
        write_u32(&mut stub, 1);
        write_u32(&mut stub, 1);
        write_u32(&mut stub, 0x0002_0008);
        write_u32(&mut stub, 1);
        write_u32(&mut stub, 0x0002_000c);
        write_u32(&mut stub, 1);
        write_u32(&mut stub, 0x0002_0010);
        write_u32(&mut stub, 0x8000_0003);
        write_u32(&mut stub, 0x0002_0014);
        encode_conformant_varying_wchar(&mut stub, "IPC$", true);
        encode_conformant_varying_wchar(&mut stub, "IPC Service (gravenas server)", true);
        write_u32(&mut stub, 0x0002_0018);
        write_u32(&mut stub, 1);
        write_u32(&mut stub, 0x0002_0018);
        write_u32(&mut stub, 0);
        write_u32(&mut stub, 0);

        let names = decode_netr_share_enum_out(&stub).expect("decode app2 response");
        assert!(names.is_empty(), "IPC$ is hidden and filtered");
    }

    #[test]
    fn decode_rejects_nonzero_return_status() {
        let mut stub = Vec::new();
        write_u32(&mut stub, 1);
        write_u32(&mut stub, 1);
        write_u32(&mut stub, 0);
        write_u32(&mut stub, 0);
        write_u32(&mut stub, 0);
        write_u32(&mut stub, 0x0000_0005);

        let err = decode_netr_share_enum_out(&stub).expect_err("non-zero status");
        assert!(err.contains("0x00000005"));
    }
}
