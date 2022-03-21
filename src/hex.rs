pub fn to(addr: &[u8]) -> String {
  addr.iter().map(|byte| format!["{:x}", byte]).collect::<Vec<String>>().join("")
}

pub fn from(s: &str) -> Option<Vec<u8>> {
  let mut result = Vec::with_capacity((s.len()+1)/2);
  for i in 0..(s.len()+1)/2 {
    if let Ok(b) =  u8::from_str_radix(&s[i*2..=(i*2+1).min(s.len())], 16) {
      result.push(b);
    } else {
      return None;
    }
  }
  Some(result)
}
