//! Binary codec for JD (Job Declaration) protocol messages
//!
//! Wire format follows SRI conventions:
//! - Little-endian integers
//! - Strings: u16 length prefix + UTF-8 bytes
//! - Variable-length byte arrays: u16 length prefix (tokens) or u32 (coinbase_tx)
//! - Fixed arrays: written directly without length prefix

use crate::error::{JdServerError, Result};
use crate::messages::{
    message_types, AllocateMiningJobToken, AllocateMiningJobTokenSuccess, JobDeclarationMode,
    PushSolution, SetCustomMiningJob, SetCustomMiningJobError, SetCustomMiningJobErrorCode,
    SetCustomMiningJobSuccess,
};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Cursor, Read, Write};
use zcash_mining_protocol::codec::MessageFrame;

/// Extension type for JD protocol (0 for standard)
const JD_EXTENSION_TYPE: u16 = 0;

/// Helper to read a u16-prefixed string
fn read_string(cursor: &mut Cursor<&[u8]>) -> Result<String> {
    let len = cursor
        .read_u16::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let mut buf = vec![0u8; len as usize];
    cursor
        .read_exact(&mut buf)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    String::from_utf8(buf).map_err(|e| JdServerError::Protocol(e.to_string()))
}

/// Helper to write a u16-prefixed string
fn write_string(payload: &mut Vec<u8>, s: &str) {
    payload
        .write_u16::<LittleEndian>(s.len() as u16)
        .unwrap();
    payload.write_all(s.as_bytes()).unwrap();
}

/// Helper to read a u16-prefixed byte vector
fn read_bytes_u16(cursor: &mut Cursor<&[u8]>) -> Result<Vec<u8>> {
    let len = cursor
        .read_u16::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let mut buf = vec![0u8; len as usize];
    cursor
        .read_exact(&mut buf)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    Ok(buf)
}

/// Helper to write a u16-prefixed byte vector
fn write_bytes_u16(payload: &mut Vec<u8>, data: &[u8]) {
    payload
        .write_u16::<LittleEndian>(data.len() as u16)
        .unwrap();
    payload.write_all(data).unwrap();
}

/// Helper to read a u32-prefixed byte vector
fn read_bytes_u32(cursor: &mut Cursor<&[u8]>) -> Result<Vec<u8>> {
    let len = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let mut buf = vec![0u8; len as usize];
    cursor
        .read_exact(&mut buf)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    Ok(buf)
}

/// Helper to write a u32-prefixed byte vector
fn write_bytes_u32(payload: &mut Vec<u8>, data: &[u8]) {
    payload
        .write_u32::<LittleEndian>(data.len() as u32)
        .unwrap();
    payload.write_all(data).unwrap();
}

/// Encode an AllocateMiningJobToken message
pub fn encode_allocate_token(msg: &AllocateMiningJobToken) -> Result<Vec<u8>> {
    let mut payload = Vec::new();

    payload
        .write_u32::<LittleEndian>(msg.request_id)
        .unwrap();
    write_string(&mut payload, &msg.user_identifier);
    payload.write_u8(msg.requested_mode.as_u8()).unwrap();

    let frame = MessageFrame {
        extension_type: JD_EXTENSION_TYPE,
        msg_type: message_types::ALLOCATE_MINING_JOB_TOKEN,
        length: payload.len() as u32,
    };

    let mut result = frame.encode().to_vec();
    result.extend(payload);
    Ok(result)
}

/// Decode an AllocateMiningJobToken message
pub fn decode_allocate_token(data: &[u8]) -> Result<AllocateMiningJobToken> {
    let frame = MessageFrame::decode(data)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    if frame.msg_type != message_types::ALLOCATE_MINING_JOB_TOKEN {
        return Err(JdServerError::Protocol(format!(
            "Invalid message type: expected 0x{:02x}, got 0x{:02x}",
            message_types::ALLOCATE_MINING_JOB_TOKEN,
            frame.msg_type
        )));
    }

    let payload = &data[MessageFrame::HEADER_SIZE..];
    let mut cursor = Cursor::new(payload);

    let request_id = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let user_identifier = read_string(&mut cursor)?;

    // Read requested_mode if present, default to CoinbaseOnly for backward compatibility
    let requested_mode = match cursor.read_u8() {
        Ok(byte) => JobDeclarationMode::from_u8(byte).unwrap_or(JobDeclarationMode::CoinbaseOnly),
        Err(_) => JobDeclarationMode::CoinbaseOnly,
    };

    Ok(AllocateMiningJobToken {
        request_id,
        user_identifier,
        requested_mode,
    })
}

/// Encode an AllocateMiningJobTokenSuccess message
pub fn encode_allocate_token_success(msg: &AllocateMiningJobTokenSuccess) -> Result<Vec<u8>> {
    let mut payload = Vec::new();

    payload
        .write_u32::<LittleEndian>(msg.request_id)
        .unwrap();
    write_bytes_u16(&mut payload, &msg.mining_job_token);
    write_bytes_u16(&mut payload, &msg.coinbase_output);
    payload
        .write_u32::<LittleEndian>(msg.coinbase_output_max_additional_size)
        .unwrap();
    payload
        .write_u8(if msg.async_mining_allowed { 1 } else { 0 })
        .unwrap();
    payload.write_u8(msg.granted_mode.as_u8()).unwrap();

    let frame = MessageFrame {
        extension_type: JD_EXTENSION_TYPE,
        msg_type: message_types::ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
        length: payload.len() as u32,
    };

    let mut result = frame.encode().to_vec();
    result.extend(payload);
    Ok(result)
}

/// Decode an AllocateMiningJobTokenSuccess message
pub fn decode_allocate_token_success(data: &[u8]) -> Result<AllocateMiningJobTokenSuccess> {
    let frame = MessageFrame::decode(data)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    if frame.msg_type != message_types::ALLOCATE_MINING_JOB_TOKEN_SUCCESS {
        return Err(JdServerError::Protocol(format!(
            "Invalid message type: expected 0x{:02x}, got 0x{:02x}",
            message_types::ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
            frame.msg_type
        )));
    }

    let payload = &data[MessageFrame::HEADER_SIZE..];
    let mut cursor = Cursor::new(payload);

    let request_id = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let mining_job_token = read_bytes_u16(&mut cursor)?;
    let coinbase_output = read_bytes_u16(&mut cursor)?;
    let coinbase_output_max_additional_size = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let async_mining_allowed = cursor
        .read_u8()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?
        != 0;

    // Read granted_mode if present, default to CoinbaseOnly for backward compatibility
    let granted_mode = match cursor.read_u8() {
        Ok(byte) => JobDeclarationMode::from_u8(byte).unwrap_or(JobDeclarationMode::CoinbaseOnly),
        Err(_) => JobDeclarationMode::CoinbaseOnly,
    };

    Ok(AllocateMiningJobTokenSuccess {
        request_id,
        mining_job_token,
        coinbase_output,
        coinbase_output_max_additional_size,
        async_mining_allowed,
        granted_mode,
    })
}

/// Encode a SetCustomMiningJob message
pub fn encode_set_custom_job(msg: &SetCustomMiningJob) -> Result<Vec<u8>> {
    let mut payload = Vec::new();

    payload.write_u32::<LittleEndian>(msg.channel_id).unwrap();
    payload.write_u32::<LittleEndian>(msg.request_id).unwrap();
    write_bytes_u16(&mut payload, &msg.mining_job_token);
    payload.write_u32::<LittleEndian>(msg.version).unwrap();
    payload.write_all(&msg.prev_hash).unwrap();
    payload.write_all(&msg.merkle_root).unwrap();
    payload.write_all(&msg.block_commitments).unwrap();
    write_bytes_u32(&mut payload, &msg.coinbase_tx);
    payload.write_u32::<LittleEndian>(msg.time).unwrap();
    payload.write_u32::<LittleEndian>(msg.bits).unwrap();

    let frame = MessageFrame {
        extension_type: JD_EXTENSION_TYPE,
        msg_type: message_types::SET_CUSTOM_MINING_JOB,
        length: payload.len() as u32,
    };

    let mut result = frame.encode().to_vec();
    result.extend(payload);
    Ok(result)
}

/// Decode a SetCustomMiningJob message
pub fn decode_set_custom_job(data: &[u8]) -> Result<SetCustomMiningJob> {
    let frame = MessageFrame::decode(data)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    if frame.msg_type != message_types::SET_CUSTOM_MINING_JOB {
        return Err(JdServerError::Protocol(format!(
            "Invalid message type: expected 0x{:02x}, got 0x{:02x}",
            message_types::SET_CUSTOM_MINING_JOB,
            frame.msg_type
        )));
    }

    let payload = &data[MessageFrame::HEADER_SIZE..];
    let mut cursor = Cursor::new(payload);

    let channel_id = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let request_id = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let mining_job_token = read_bytes_u16(&mut cursor)?;
    let version = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;

    let mut prev_hash = [0u8; 32];
    cursor
        .read_exact(&mut prev_hash)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;

    let mut merkle_root = [0u8; 32];
    cursor
        .read_exact(&mut merkle_root)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;

    let mut block_commitments = [0u8; 32];
    cursor
        .read_exact(&mut block_commitments)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;

    let coinbase_tx = read_bytes_u32(&mut cursor)?;
    let time = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let bits = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;

    Ok(SetCustomMiningJob {
        channel_id,
        request_id,
        mining_job_token,
        version,
        prev_hash,
        merkle_root,
        block_commitments,
        coinbase_tx,
        time,
        bits,
    })
}

/// Encode a SetCustomMiningJobSuccess message
pub fn encode_set_custom_job_success(msg: &SetCustomMiningJobSuccess) -> Result<Vec<u8>> {
    let mut payload = Vec::new();

    payload.write_u32::<LittleEndian>(msg.channel_id).unwrap();
    payload.write_u32::<LittleEndian>(msg.request_id).unwrap();
    payload.write_u32::<LittleEndian>(msg.job_id).unwrap();

    let frame = MessageFrame {
        extension_type: JD_EXTENSION_TYPE,
        msg_type: message_types::SET_CUSTOM_MINING_JOB_SUCCESS,
        length: payload.len() as u32,
    };

    let mut result = frame.encode().to_vec();
    result.extend(payload);
    Ok(result)
}

/// Decode a SetCustomMiningJobSuccess message
pub fn decode_set_custom_job_success(data: &[u8]) -> Result<SetCustomMiningJobSuccess> {
    let frame = MessageFrame::decode(data)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    if frame.msg_type != message_types::SET_CUSTOM_MINING_JOB_SUCCESS {
        return Err(JdServerError::Protocol(format!(
            "Invalid message type: expected 0x{:02x}, got 0x{:02x}",
            message_types::SET_CUSTOM_MINING_JOB_SUCCESS,
            frame.msg_type
        )));
    }

    let payload = &data[MessageFrame::HEADER_SIZE..];
    let mut cursor = Cursor::new(payload);

    let channel_id = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let request_id = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let job_id = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;

    Ok(SetCustomMiningJobSuccess {
        channel_id,
        request_id,
        job_id,
    })
}

/// Encode a SetCustomMiningJobError message
pub fn encode_set_custom_job_error(msg: &SetCustomMiningJobError) -> Result<Vec<u8>> {
    let mut payload = Vec::new();

    payload.write_u32::<LittleEndian>(msg.channel_id).unwrap();
    payload.write_u32::<LittleEndian>(msg.request_id).unwrap();
    payload.write_u8(msg.error_code.as_u8()).unwrap();
    write_string(&mut payload, &msg.error_message);

    let frame = MessageFrame {
        extension_type: JD_EXTENSION_TYPE,
        msg_type: message_types::SET_CUSTOM_MINING_JOB_ERROR,
        length: payload.len() as u32,
    };

    let mut result = frame.encode().to_vec();
    result.extend(payload);
    Ok(result)
}

/// Decode a SetCustomMiningJobError message
pub fn decode_set_custom_job_error(data: &[u8]) -> Result<SetCustomMiningJobError> {
    let frame = MessageFrame::decode(data)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    if frame.msg_type != message_types::SET_CUSTOM_MINING_JOB_ERROR {
        return Err(JdServerError::Protocol(format!(
            "Invalid message type: expected 0x{:02x}, got 0x{:02x}",
            message_types::SET_CUSTOM_MINING_JOB_ERROR,
            frame.msg_type
        )));
    }

    let payload = &data[MessageFrame::HEADER_SIZE..];
    let mut cursor = Cursor::new(payload);

    let channel_id = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let request_id = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let error_code_byte = cursor
        .read_u8()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let error_code = SetCustomMiningJobErrorCode::from_u8(error_code_byte)
        .ok_or_else(|| JdServerError::Protocol(format!("Unknown error code: 0x{:02x}", error_code_byte)))?;
    let error_message = read_string(&mut cursor)?;

    Ok(SetCustomMiningJobError {
        channel_id,
        request_id,
        error_code,
        error_message,
    })
}

/// Encode a PushSolution message
pub fn encode_push_solution(msg: &PushSolution) -> Result<Vec<u8>> {
    let mut payload = Vec::new();

    payload.write_u32::<LittleEndian>(msg.channel_id).unwrap();
    payload.write_u32::<LittleEndian>(msg.job_id).unwrap();
    payload.write_u32::<LittleEndian>(msg.version).unwrap();
    payload.write_u32::<LittleEndian>(msg.time).unwrap();
    payload.write_all(&msg.nonce).unwrap();
    payload.write_all(&msg.solution).unwrap();

    let frame = MessageFrame {
        extension_type: JD_EXTENSION_TYPE,
        msg_type: message_types::PUSH_SOLUTION,
        length: payload.len() as u32,
    };

    let mut result = frame.encode().to_vec();
    result.extend(payload);
    Ok(result)
}

/// Decode a PushSolution message
pub fn decode_push_solution(data: &[u8]) -> Result<PushSolution> {
    let frame = MessageFrame::decode(data)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    if frame.msg_type != message_types::PUSH_SOLUTION {
        return Err(JdServerError::Protocol(format!(
            "Invalid message type: expected 0x{:02x}, got 0x{:02x}",
            message_types::PUSH_SOLUTION,
            frame.msg_type
        )));
    }

    let payload = &data[MessageFrame::HEADER_SIZE..];
    let mut cursor = Cursor::new(payload);

    let channel_id = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let job_id = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let version = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;
    let time = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;

    let mut nonce = [0u8; 32];
    cursor
        .read_exact(&mut nonce)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;

    let mut solution = [0u8; 1344];
    cursor
        .read_exact(&mut solution)
        .map_err(|e| JdServerError::Protocol(e.to_string()))?;

    Ok(PushSolution {
        channel_id,
        job_id,
        version,
        time,
        nonce,
        solution,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_token_roundtrip() {
        let original = AllocateMiningJobToken {
            request_id: 42,
            user_identifier: "test-miner-001".to_string(),
            requested_mode: JobDeclarationMode::CoinbaseOnly,
        };

        let encoded = encode_allocate_token(&original).unwrap();
        let decoded = decode_allocate_token(&encoded).unwrap();

        assert_eq!(original.request_id, decoded.request_id);
        assert_eq!(original.user_identifier, decoded.user_identifier);
        assert_eq!(original.requested_mode, decoded.requested_mode);
    }

    #[test]
    fn test_allocate_token_full_template_mode_roundtrip() {
        let original = AllocateMiningJobToken {
            request_id: 42,
            user_identifier: "test-miner-001".to_string(),
            requested_mode: JobDeclarationMode::FullTemplate,
        };

        let encoded = encode_allocate_token(&original).unwrap();
        let decoded = decode_allocate_token(&encoded).unwrap();

        assert_eq!(original.request_id, decoded.request_id);
        assert_eq!(original.user_identifier, decoded.user_identifier);
        assert_eq!(original.requested_mode, decoded.requested_mode);
    }

    #[test]
    fn test_allocate_token_success_roundtrip() {
        let original = AllocateMiningJobTokenSuccess {
            request_id: 42,
            mining_job_token: vec![0x01, 0x02, 0x03, 0x04],
            coinbase_output: vec![0x76, 0xa9, 0x14, 0xde, 0xad, 0xbe, 0xef],
            coinbase_output_max_additional_size: 1000,
            async_mining_allowed: true,
            granted_mode: JobDeclarationMode::CoinbaseOnly,
        };

        let encoded = encode_allocate_token_success(&original).unwrap();
        let decoded = decode_allocate_token_success(&encoded).unwrap();

        assert_eq!(original.request_id, decoded.request_id);
        assert_eq!(original.mining_job_token, decoded.mining_job_token);
        assert_eq!(original.coinbase_output, decoded.coinbase_output);
        assert_eq!(
            original.coinbase_output_max_additional_size,
            decoded.coinbase_output_max_additional_size
        );
        assert_eq!(original.async_mining_allowed, decoded.async_mining_allowed);
        assert_eq!(original.granted_mode, decoded.granted_mode);
    }

    #[test]
    fn test_allocate_token_success_full_template_mode_roundtrip() {
        let original = AllocateMiningJobTokenSuccess {
            request_id: 42,
            mining_job_token: vec![0x01, 0x02, 0x03, 0x04],
            coinbase_output: vec![0x76, 0xa9, 0x14, 0xde, 0xad, 0xbe, 0xef],
            coinbase_output_max_additional_size: 1000,
            async_mining_allowed: true,
            granted_mode: JobDeclarationMode::FullTemplate,
        };

        let encoded = encode_allocate_token_success(&original).unwrap();
        let decoded = decode_allocate_token_success(&encoded).unwrap();

        assert_eq!(original.request_id, decoded.request_id);
        assert_eq!(original.mining_job_token, decoded.mining_job_token);
        assert_eq!(original.coinbase_output, decoded.coinbase_output);
        assert_eq!(
            original.coinbase_output_max_additional_size,
            decoded.coinbase_output_max_additional_size
        );
        assert_eq!(original.async_mining_allowed, decoded.async_mining_allowed);
        assert_eq!(original.granted_mode, decoded.granted_mode);
    }

    #[test]
    fn test_set_custom_job_roundtrip() {
        let original = SetCustomMiningJob {
            channel_id: 1,
            request_id: 100,
            mining_job_token: vec![0xaa, 0xbb, 0xcc],
            version: 5,
            prev_hash: [0x11; 32],
            merkle_root: [0x22; 32],
            block_commitments: [0x33; 32],
            coinbase_tx: vec![0x01, 0x00, 0x00, 0x00, 0x01, 0x00],
            time: 1700000000,
            bits: 0x1d00ffff,
        };

        let encoded = encode_set_custom_job(&original).unwrap();
        let decoded = decode_set_custom_job(&encoded).unwrap();

        assert_eq!(original.channel_id, decoded.channel_id);
        assert_eq!(original.request_id, decoded.request_id);
        assert_eq!(original.mining_job_token, decoded.mining_job_token);
        assert_eq!(original.version, decoded.version);
        assert_eq!(original.prev_hash, decoded.prev_hash);
        assert_eq!(original.merkle_root, decoded.merkle_root);
        assert_eq!(original.block_commitments, decoded.block_commitments);
        assert_eq!(original.coinbase_tx, decoded.coinbase_tx);
        assert_eq!(original.time, decoded.time);
        assert_eq!(original.bits, decoded.bits);
    }

    #[test]
    fn test_set_custom_job_success_roundtrip() {
        let original = SetCustomMiningJobSuccess {
            channel_id: 1,
            request_id: 100,
            job_id: 42,
        };

        let encoded = encode_set_custom_job_success(&original).unwrap();
        let decoded = decode_set_custom_job_success(&encoded).unwrap();

        assert_eq!(original.channel_id, decoded.channel_id);
        assert_eq!(original.request_id, decoded.request_id);
        assert_eq!(original.job_id, decoded.job_id);
    }

    #[test]
    fn test_set_custom_job_error_roundtrip() {
        let original = SetCustomMiningJobError {
            channel_id: 1,
            request_id: 100,
            error_code: SetCustomMiningJobErrorCode::InvalidToken,
            error_message: "Token is invalid".to_string(),
        };

        let encoded = encode_set_custom_job_error(&original).unwrap();
        let decoded = decode_set_custom_job_error(&encoded).unwrap();

        assert_eq!(original.channel_id, decoded.channel_id);
        assert_eq!(original.request_id, decoded.request_id);
        assert_eq!(original.error_code, decoded.error_code);
        assert_eq!(original.error_message, decoded.error_message);
    }

    #[test]
    fn test_push_solution_roundtrip() {
        let original = PushSolution {
            channel_id: 1,
            job_id: 42,
            version: 5,
            time: 1700000000,
            nonce: [0x55; 32],
            solution: [0x66; 1344],
        };

        let encoded = encode_push_solution(&original).unwrap();
        let decoded = decode_push_solution(&encoded).unwrap();

        assert_eq!(original.channel_id, decoded.channel_id);
        assert_eq!(original.job_id, decoded.job_id);
        assert_eq!(original.version, decoded.version);
        assert_eq!(original.time, decoded.time);
        assert_eq!(original.nonce, decoded.nonce);
        assert_eq!(original.solution, decoded.solution);
    }

    #[test]
    fn test_frame_header_size() {
        // Verify that our messages include the 6-byte header
        let token = AllocateMiningJobToken {
            request_id: 1,
            user_identifier: "x".to_string(),
            requested_mode: JobDeclarationMode::CoinbaseOnly,
        };
        let encoded = encode_allocate_token(&token).unwrap();

        // Should have 6-byte header + 4-byte request_id + 2-byte len + 1-byte string + 1-byte mode
        assert!(encoded.len() >= MessageFrame::HEADER_SIZE);

        // First 2 bytes are extension_type (0)
        assert_eq!(encoded[0], 0);
        assert_eq!(encoded[1], 0);
        // Third byte is message type
        assert_eq!(encoded[2], message_types::ALLOCATE_MINING_JOB_TOKEN);
    }

    #[test]
    fn test_all_error_codes_roundtrip() {
        let codes = [
            SetCustomMiningJobErrorCode::InvalidToken,
            SetCustomMiningJobErrorCode::TokenExpired,
            SetCustomMiningJobErrorCode::InvalidCoinbase,
            SetCustomMiningJobErrorCode::CoinbaseConstraintViolation,
            SetCustomMiningJobErrorCode::StalePrevHash,
            SetCustomMiningJobErrorCode::InvalidMerkleRoot,
            SetCustomMiningJobErrorCode::InvalidVersion,
            SetCustomMiningJobErrorCode::InvalidBits,
            SetCustomMiningJobErrorCode::ServerOverloaded,
            SetCustomMiningJobErrorCode::Other,
        ];

        for code in codes {
            let original = SetCustomMiningJobError {
                channel_id: 1,
                request_id: 1,
                error_code: code,
                error_message: format!("Error: {}", code),
            };

            let encoded = encode_set_custom_job_error(&original).unwrap();
            let decoded = decode_set_custom_job_error(&encoded).unwrap();

            assert_eq!(original.error_code, decoded.error_code);
        }
    }
}
