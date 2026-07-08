//! protocol/rdb/crc64.rs - CRC64 实现
//!
//! 本文件实现 Redis 使用的 CRC64 校验算法（Jones 多项式）。
//! 用于 DUMP 格式的校验和计算。

/// CRC64 查找表（Jones 多项式）
/// 从 Redis 源代码中提取
const CRC64_TAB: [u64; 256] = [
    0x0000000000000000u64, 0x7ad870c830358979u64, 0xf5b0e190606b12f2u64, 0x8f689158505e9b8bu64,
    0xc038e5739841b68fu64, 0xbae095bba8743ff6u64, 0x358804e3f82aa47du64, 0x4f50742bc81f2d04u64,
    0xab28ecb46814fe75u64, 0xd1f09c7c5821770cu64, 0x5e980d24087fec87u64, 0x24407dec384a65feu64,
    0x6b1009c7f05548fau64, 0x11c8790fc060c183u64, 0x9ea0e857903e5a08u64, 0xe478989fa00bd371u64,
    0x7d08ff3b88be6f81u64, 0x07d08ff3b88be6f8u64, 0x88b81eabe8d57d73u64, 0xf2606e63d8e0f40au64,
    0xbd301a4810ffd90eu64, 0xc7e86a8020ca5077u64, 0x4880fbd87094cbfcu64, 0x32588b1040a14285u64,
    0xd620138fe0aa91f4u64, 0xacf86347d09f188du64, 0x2390f21f80c18306u64, 0x594882d7b0f40a7fu64,
    0x1618f6fc78eb277bu64, 0x6cc0863448deae02u64, 0xe3a8176c18803589u64, 0x997067a428b5bcf0u64,
    0xfa11fe77117cdf02u64, 0x80c98ebf2149567bu64, 0x0fa11fe77117cdf0u64, 0x75796f2f41224489u64,
    0x3a291b04893d698du64, 0x40f16bccb908e0f4u64, 0xcf99fa94e9567b7fu64, 0xb5418a5cd963f206u64,
    0x513912c379682177u64, 0x2be1620b495da80eu64, 0xa489f35319033385u64, 0xde51839b2936bafcu64,
    0x9101f7b0e12997f8u64, 0xebd98778d11c1e81u64, 0x64b116208142850au64, 0x1e6966e8b1770c73u64,
    0x8719014c99c2b083u64, 0xfdc17184a9f739fau64, 0x72a9e0dcf9a9a271u64, 0x08719014c99c2b08u64,
    0x4721e43f0183060cu64, 0x3df994f731b68f75u64, 0xb29105af61e814feu64, 0xc849756751dd9d87u64,
    0x2c31edf8f1d64ef6u64, 0x56e99d30c1e3c78fu64, 0xd9810c6891bd5c04u64, 0xa3597ca0a188d57du64,
    0xec09088b6997f879u64, 0x96d1784359a27100u64, 0x19b9e91b09fcea8bu64, 0x636199d339c963f2u64,
    0xdf7adabd7a6e2d6fu64, 0xa5a2aa754a5ba416u64, 0x2aca3b2d1a053f9du64, 0x50124be52a30b6e4u64,
    0x1f423fcee22f9be0u64, 0x659a4f06d21a1299u64, 0xeaf2de5e82448912u64, 0x902aae96b271006bu64,
    0x74523609127ad31au64, 0x0e8a46c1224f5a63u64, 0x81e2d7997211c1e8u64, 0xfb3aa75142244891u64,
    0xb46ad37a8a3b6595u64, 0xceb2a3b2ba0eececu64, 0x41da32eaea507767u64, 0x3b024222da65fe1eu64,
    0xa2722586f2d042eeu64, 0xd8aa554ec2e5cb97u64, 0x57c2c41692bb501cu64, 0x2d1ab4dea28ed965u64,
    0x624ac0f56a91f461u64, 0x1892b03d5aa47d18u64, 0x97fa21650afae693u64, 0xed2251ad3acf6feau64,
    0x095ac9329ac4bc9bu64, 0x7382b9faaaf135e2u64, 0xfcea28a2faafae69u64, 0x8632586aca9a2710u64,
    0xc9622c4102850a14u64, 0xb3ba5c8932b0836du64, 0x3cd2cdd162ee18e6u64, 0x460abd1952db919fu64,
    0x256b24ca6b12f26du64, 0x5fb354025b277b14u64, 0xd0dbc55a0b79e09fu64, 0xaa03b5923b4c69e6u64,
    0xe553c1b9f35344e2u64, 0x9f8bb171c366cd9bu64, 0x10e3202993385610u64, 0x6a3b50e1a30ddf69u64,
    0x8e43c87e03060c18u64, 0xf49bb8b633338561u64, 0x7bf329ee636d1eeau64, 0x012b592653589793u64,
    0x4e7b2d0d9b47ba97u64, 0x34a35dc5ab7233eeu64, 0xbbcbcc9dfb2ca865u64, 0xc113bc55cb19211cu64,
    0x5863dbf1e3ac9decu64, 0x22bbab39d3991495u64, 0xadd33a6183c78f1eu64, 0xd70b4aa9b3f20667u64,
    0x985b3e827bed2b63u64, 0xe2834e4a4bd8a21au64, 0x6debdf121b863991u64, 0x1733afda2bb3b0e8u64,
    0xf34b37458bb86399u64, 0x8993478dbb8deae0u64, 0x06fbd6d5ebd3716bu64, 0x7c23a61ddbe6f812u64,
    0x3373d23613f9d516u64, 0x49aba2fe23cc5c6fu64, 0xc6c333a67392c7e4u64, 0xbc1b436e43a74e9du64,
    0x95ac9329ac4bc9b5u64, 0xef74e3e19c7e40ccu64, 0x601c72b9cc20db47u64, 0x1ac40271fc15523eu64,
    0x5594765a340a7f3au64, 0x2f4c0692043ff643u64, 0xa02497ca54616dc8u64, 0xdafce7026454e4b1u64,
    0x3e847f9dc45f37c0u64, 0x445c0f55f46abeb9u64, 0xcb349e0da4342532u64, 0xb1eceec59401ac4bu64,
    0xfebc9aee5c1e814fu64, 0x8464ea266c2b0836u64, 0x0b0c7b7e3c7593bdu64, 0x71d40bb60c401ac4u64,
    0xe8a46c1224f5a634u64, 0x927c1cda14c02f4du64, 0x1d148d82449eb4c6u64, 0x67ccfd4a74ab3dbfu64,
    0x289c8961bcb410bbu64, 0x5244f9a98c8199c2u64, 0xdd2c68f1dcdf0249u64, 0xa7f41839ecea8b30u64,
    0x438c80a64ce15841u64, 0x3954f06e7cd4d138u64, 0xb63c61362c8a4ab3u64, 0xcce411fe1cbfc3cau64,
    0x83b465d5d4a0eeceu64, 0xf96c151de49567b7u64, 0x76048445b4cbfc3cu64, 0x0cdcf48d84fe7545u64,
    0x6fbd6d5ebd3716b7u64, 0x15651d968d029fceu64, 0x9a0d8ccedd5c0445u64, 0xe0d5fc06ed698d3cu64,
    0xaf85882d2576a038u64, 0xd55df8e515432941u64, 0x5a3569bd451db2cau64, 0x20ed197575283bb3u64,
    0xc49581ead523e8c2u64, 0xbe4df122e51661bbu64, 0x3125607ab548fa30u64, 0x4bfd10b2857d7349u64,
    0x04ad64994d625e4du64, 0x7e7514517d57d734u64, 0xf11d85092d094cbfu64, 0x8bc5f5c11d3cc5c6u64,
    0x12b5926535897936u64, 0x686de2ad05bcf04fu64, 0xe70573f555e26bc4u64, 0x9ddd033d65d7e2bdu64,
    0xd28d7716adc8cfb9u64, 0xa85507de9dfd46c0u64, 0x273d9686cda3dd4bu64, 0x5de5e64efd965432u64,
    0xb99d7ed15d9d8743u64, 0xc3450e196da80e3au64, 0x4c2d9f413df695b1u64, 0x36f5ef890dc31cc8u64,
    0x79a59ba2c5dc31ccu64, 0x037deb6af5e9b8b5u64, 0x8c157a32a5b7233eu64, 0xf6cd0afa9582aa47u64,
    0x4ad64994d625e4dau64, 0x300e395ce6106da3u64, 0xbf66a804b64ef628u64, 0xc5bed8cc867b7f51u64,
    0x8aeeace74e645255u64, 0xf036dc2f7e51db2cu64, 0x7f5e4d772e0f40a7u64, 0x05863dbf1e3ac9deu64,
    0xe1fea520be311aafu64, 0x9b26d5e88e0493d6u64, 0x144e44b0de5a085du64, 0x6e963478ee6f8124u64,
    0x21c640532670ac20u64, 0x5b1e309b16452559u64, 0xd476a1c3461bbed2u64, 0xaeaed10b762e37abu64,
    0x37deb6af5e9b8b5bu64, 0x4d06c6676eae0222u64, 0xc26e573f3ef099a9u64, 0xb8b627f70ec510d0u64,
    0xf7e653dcc6da3dd4u64, 0x8d3e2314f6efb4adu64, 0x0256b24ca6b12f26u64, 0x788ec2849684a65fu64,
    0x9cf65a1b368f752eu64, 0xe62e2ad306bafc57u64, 0x6946bb8b56e467dcu64, 0x139ecb4366d1eea5u64,
    0x5ccebf68aecec3a1u64, 0x2616cfa09efb4ad8u64, 0xa97e5ef8cea5d153u64, 0xd3a62e30fe90582au64,
    0xb0c7b7e3c7593bd8u64, 0xca1fc72bf76cb2a1u64, 0x45775673a732292au64, 0x3faf26bb9707a053u64,
    0x70ff52905f188d57u64, 0x0a2722586f2d042eu64, 0x854fb3003f739fa5u64, 0xff97c3c80f4616dcu64,
    0x1bef5b57af4dc5adu64, 0x61372b9f9f784cd4u64, 0xee5fbac7cf26d75fu64, 0x9487ca0fff135e26u64,
    0xdbd7be24370c7322u64, 0xa10fceec0739fa5bu64, 0x2e675fb4576761d0u64, 0x54bf2f7c6752e8a9u64,
    0xcdcf48d84fe75459u64, 0xb71738107fd2dd20u64, 0x387fa9482f8c46abu64, 0x42a7d9801fb9cfd2u64,
    0x0df7adabd7a6e2d6u64, 0x772fdd63e7936bafu64, 0xf8474c3bb7cdf024u64, 0x829f3cf387f8795du64,
    0x66e7a46c27f3aa2cu64, 0x1c3fd4a417c62355u64, 0x935745fc4798b8deu64, 0xe98f353477ad31a7u64,
    0xa6df411fbfb21ca3u64, 0xdc0731d78f8795dau64, 0x536fa08fdfd90e51u64, 0x29b7d047efec8728u64,
];

/// 计算 CRC64 校验和
///
/// # 参数
/// - data: 要计算校验和的数据
///
/// # 返回
/// CRC64 校验和值
pub fn crc64(data: &[u8]) -> u64 {
    let mut crc: u64 = 0;
    for &byte in data {
        let index = ((crc ^ byte as u64) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC64_TAB[index];
    }
    crc
}

/// 计算 CRC64 校验和（带初始值）
///
/// # 参数
/// - data: 要计算校验和的数据
/// - initial: 初始 CRC 值
///
/// # 返回
/// CRC64 校验和值
pub fn crc64_with_initial(data: &[u8], initial: u64) -> u64 {
    let mut crc = initial;
    for &byte in data {
        let index = ((crc ^ byte as u64) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC64_TAB[index];
    }
    crc
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc64_empty() {
        assert_eq!(crc64(&[]), 0);
    }

    #[test]
    fn test_crc64_simple() {
        let data = b"hello";
        let result = crc64(data);
        // 确保计算结果不为 0
        assert_ne!(result, 0);
    }

    #[test]
    fn test_crc64_consistency() {
        let data = b"test data";
        let result1 = crc64(data);
        let result2 = crc64(data);
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_crc64_different_data() {
        let data1 = b"hello";
        let data2 = b"world";
        assert_ne!(crc64(data1), crc64(data2));
    }

    #[test]
    fn test_crc64_with_initial() {
        let data = b"test";
        let result1 = crc64_with_initial(data, 0);
        let result2 = crc64(data);
        assert_eq!(result1, result2);
    }

    /// 测试 Redis CRC64 标准测试向量
    #[test]
    fn test_crc64_redis_standard() {
        // Redis 标准测试向量: "123456789" -> 0xe9c6d914c4b8d9ca
        let data = b"123456789";
        let result = crc64(data);
        assert_eq!(result, 0xe9c6d914c4b8d9ca, "CRC64 of '123456789' should be 0xe9c6d914c4b8d9ca");
    }
}

