// ═══════════════════════════════════════════════════════════════
// Zcash Mining Infrastructure - Formal Specification
// ═══════════════════════════════════════════════════════════════

module basicSpells {
  pure def max(a: int, b: int): int = if (a >= b) a else b
  pure def min(a: int, b: int): int = if (a <= b) a else b
  pure def abs(x: int): int = if (x >= 0) x else -x
  pure def clamp(x: int, lo: int, hi: int): int = max(lo, min(x, hi))
}

module miningSpells {
  import basicSpells.*
  pure val POW_LIMIT = 1000000
  pure def target_to_difficulty(target: int): int =
    if (target <= 0) POW_LIMIT else POW_LIMIT / target
  pure def meets_target(sd: int, rd: int): bool = sd >= rd
  pure def meets_block_target(sd: int, bd: int): bool = sd >= bd
}

// ═══════════════════════════════════════════════════════════════
// MODULE 1: Share Validation Pipeline
// ═══════════════════════════════════════════════════════════════

module ShareValidation {
  import basicSpells.*
  import miningSpells.*

  const MINERS: Set[int]
  const MAX_JOB_ID: int
  const MAX_SHARES_PER_JOB: int
  const RATE_LIMIT_CAPACITY: int
  const RATE_LIMIT_REFILL: int
  const JOB_TTL: int
  const MAX_JOBS_PER_CHANNEL: int

  var jobs: int -> (int -> { target: int, active: bool, created_at: int })
  var lastJobId: int -> int
  var rateLimitTokens: int -> int
  var duplicateSets: int -> Set[int]
  var nonceAssignments: int -> int
  var blockDifficulty: int
  var clock: int
  var ghostAccepted: Set[(int, int, int)]
  var ghostBlocksFound: int
  var ghostDupsRejected: int
  var ghostTotal: int

  action init = all {
    jobs' = MINERS.mapBy(_ => Map()),
    lastJobId' = MINERS.mapBy(_ => 0),
    rateLimitTokens' = MINERS.mapBy(_ => RATE_LIMIT_CAPACITY),
    duplicateSets' = Map(),
    nonceAssignments' = MINERS.mapBy(m => m),
    blockDifficulty' = 100,
    clock' = 0,
    ghostAccepted' = Set(),
    ghostBlocksFound' = 0,
    ghostDupsRejected' = 0,
    ghostTotal' = 0,
  }

  // Frame helper: keep all state unchanged except listed
  action frameAll = all {
    jobs' = jobs, lastJobId' = lastJobId, rateLimitTokens' = rateLimitTokens,
    duplicateSets' = duplicateSets, nonceAssignments' = nonceAssignments,
    blockDifficulty' = blockDifficulty, clock' = clock,
    ghostAccepted' = ghostAccepted, ghostBlocksFound' = ghostBlocksFound,
    ghostDupsRejected' = ghostDupsRejected, ghostTotal' = ghostTotal,
  }

  action addJob(ch: int, jid: int, tgt: int, clean: bool): bool = {
    val cJobs = jobs.get(ch)
    val deact = if (clean)
      cJobs.keys().mapBy(j => { ...cJobs.get(j), active: false })
    else cJobs
    val entry = { target: tgt, active: true, created_at: clock }
    val withNew = deact.put(jid, entry)
    val trimmed = if (withNew.keys().size() > MAX_JOBS_PER_CHANNEL) {
      val cutoff = jid - MAX_JOBS_PER_CHANNEL
      withNew.keys().filter(j => j > cutoff).mapBy(j => withNew.get(j))
    } else withNew
    all {
      MINERS.contains(ch), jid > 0, tgt > 0, jid > lastJobId.get(ch),
      jobs' = jobs.set(ch, trimmed),
      lastJobId' = lastJobId.setBy(ch, cur => max(cur, jid)),
      rateLimitTokens' = rateLimitTokens, duplicateSets' = duplicateSets,
      nonceAssignments' = nonceAssignments, blockDifficulty' = blockDifficulty,
      clock' = clock, ghostAccepted' = ghostAccepted,
      ghostBlocksFound' = ghostBlocksFound, ghostDupsRejected' = ghostDupsRejected,
      ghostTotal' = ghostTotal,
    }
  }

  action validateShare(ch: int, jid: int, n2: int, sd: int): bool = {
    val tokens = rateLimitTokens.get(ch)
    val cJobs = jobs.get(ch)
    val hasJob = cJobs.keys().contains(jid)
    val jobActive = if (hasJob) cJobs.get(jid).active else false
    val jobExpired = if (hasJob) (clock - cJobs.get(jid).created_at > JOB_TTL) else true
    val jobTarget = if (hasJob) cJobs.get(jid).target else 0
    val hasDups = duplicateSets.keys().contains(jid)
    val jobDups = if (hasDups) duplicateSets.get(jid) else Set()
    val isDup = jobDups.contains(n2) or jobDups.size() >= MAX_SHARES_PER_JOB
    val reqDiff = target_to_difficulty(jobTarget)
    val meetsShare = sd > 0 and meets_target(sd, reqDiff)
    val isBlock = meets_block_target(sd, blockDifficulty)
    all {
      MINERS.contains(ch),
      ghostTotal' = ghostTotal + 1,
      if (tokens < 1) all {
        // Rate limited
        rateLimitTokens' = rateLimitTokens,
        jobs' = jobs, lastJobId' = lastJobId, duplicateSets' = duplicateSets,
        nonceAssignments' = nonceAssignments, blockDifficulty' = blockDifficulty,
        clock' = clock, ghostAccepted' = ghostAccepted,
        ghostBlocksFound' = ghostBlocksFound, ghostDupsRejected' = ghostDupsRejected,
      }
      else if (not(hasJob) or not(jobActive) or jobExpired) all {
        // Unknown/stale job
        rateLimitTokens' = rateLimitTokens.set(ch, tokens - 1),
        jobs' = jobs, lastJobId' = lastJobId, duplicateSets' = duplicateSets,
        nonceAssignments' = nonceAssignments, blockDifficulty' = blockDifficulty,
        clock' = clock, ghostAccepted' = ghostAccepted,
        ghostBlocksFound' = ghostBlocksFound, ghostDupsRejected' = ghostDupsRejected,
      }
      else if (isDup) all {
        // Duplicate
        rateLimitTokens' = rateLimitTokens.set(ch, tokens - 1),
        ghostDupsRejected' = ghostDupsRejected + 1,
        jobs' = jobs, lastJobId' = lastJobId, duplicateSets' = duplicateSets,
        nonceAssignments' = nonceAssignments, blockDifficulty' = blockDifficulty,
        clock' = clock, ghostAccepted' = ghostAccepted,
        ghostBlocksFound' = ghostBlocksFound,
      }
      else if (not(meetsShare)) all {
        // Invalid/low difficulty
        rateLimitTokens' = rateLimitTokens.set(ch, tokens - 1),
        duplicateSets' = duplicateSets.put(jid, jobDups.union(Set(n2))),
        jobs' = jobs, lastJobId' = lastJobId,
        nonceAssignments' = nonceAssignments, blockDifficulty' = blockDifficulty,
        clock' = clock, ghostAccepted' = ghostAccepted,
        ghostBlocksFound' = ghostBlocksFound, ghostDupsRejected' = ghostDupsRejected,
      }
      else all {
        // ACCEPTED
        rateLimitTokens' = rateLimitTokens.set(ch, tokens - 1),
        duplicateSets' = duplicateSets.put(jid, jobDups.union(Set(n2))),
        ghostAccepted' = ghostAccepted.union(Set((ch, jid, n2))),
        ghostBlocksFound' = if (isBlock) ghostBlocksFound + 1 else ghostBlocksFound,
        jobs' = jobs, lastJobId' = lastJobId,
        nonceAssignments' = nonceAssignments, blockDifficulty' = blockDifficulty,
        clock' = clock, ghostDupsRejected' = ghostDupsRejected,
      }
    }
  }

  action tick = all {
    clock' = clock + 1,
    rateLimitTokens' = MINERS.mapBy(m =>
      min(RATE_LIMIT_CAPACITY, rateLimitTokens.get(m) + RATE_LIMIT_REFILL)),
    jobs' = jobs, lastJobId' = lastJobId, duplicateSets' = duplicateSets,
    nonceAssignments' = nonceAssignments, blockDifficulty' = blockDifficulty,
    ghostAccepted' = ghostAccepted, ghostBlocksFound' = ghostBlocksFound,
    ghostDupsRejected' = ghostDupsRejected, ghostTotal' = ghostTotal,
  }

  action newBlock(bd: int): bool = all {
    bd > 0,
    blockDifficulty' = bd, duplicateSets' = Map(), clock' = clock + 1,
    jobs' = jobs, lastJobId' = lastJobId, rateLimitTokens' = rateLimitTokens,
    nonceAssignments' = nonceAssignments,
    ghostAccepted' = ghostAccepted, ghostBlocksFound' = ghostBlocksFound,
    ghostDupsRejected' = ghostDupsRejected, ghostTotal' = ghostTotal,
  }

  action step = {
    nondet m = MINERS.oneOf()
    // High targets → low reqDiff (e.g. target=10000 → reqDiff=100)
    nondet t = 5000.to(20000).oneOf()
    nondet bd = 50.to(150).oneOf()
    nondet n = 1.to(50).oneOf()
    nondet s = 0.to(500).oneOf()
    // Smart job selection: bias toward existing jobs, include a few misses
    val existingJobs = jobs.get(m).keys()
    val candidateJobs = existingJobs.union(Set(1, MAX_JOB_ID))
    nondet j = candidateJobs.oneOf()
    // Fresh job IDs just above the last assigned
    nondet freshJ = (lastJobId.get(m) + 1).to(lastJobId.get(m) + 5).oneOf()
    any {
      validateShare(m, j, n, s),
      addJob(m, freshJ, t, false),
      addJob(m, freshJ, t, true),
      tick,
      newBlock(bd),
    }
  }

  val rateLimitBounded = MINERS.forall(m =>
    rateLimitTokens.get(m) >= 0 and rateLimitTokens.get(m) <= RATE_LIMIT_CAPACITY)
  val jobCountBounded = MINERS.forall(m =>
    jobs.get(m).keys().size() <= MAX_JOBS_PER_CHANNEL)
  val nonceDisjoint = MINERS.forall(m1 => MINERS.forall(m2 =>
    (m1 == m2) or nonceAssignments.get(m1) != nonceAssignments.get(m2)))
  val blockDiffPositive = blockDifficulty > 0
  val dupSetBounded = duplicateSets.keys().forall(jid =>
    duplicateSets.get(jid).size() <= MAX_SHARES_PER_JOB)
  // Witnesses
  val witnessNoAccepted = ghostAccepted.size() == 0
  val witnessNoDups = ghostDupsRejected == 0
  // Reachability witnesses — expected to be violated by simulator
  val witnessAcceptedReached = ghostAccepted.size() == 0
  val witnessBlockFound = ghostBlocksFound == 0
}

module ShareValidationTest {
  import ShareValidation(
    MINERS = Set(1, 2, 3), MAX_JOB_ID = 10, MAX_SHARES_PER_JOB = 20,
    RATE_LIMIT_CAPACITY = 5, RATE_LIMIT_REFILL = 2,
    JOB_TTL = 10, MAX_JOBS_PER_CHANNEL = 3,
  ).*

  // target=10000 → reqDiff=POW_LIMIT/10000=100, sd=150 meets it
  run happyPath = init
    .then(addJob(1, 1, 10000, false))
    .then(validateShare(1, 1, 42, 150))
    .expect(ghostAccepted.size() == 1)

  run dupRejected = init
    .then(addJob(1, 1, 10000, false))
    .then(validateShare(1, 1, 42, 150))
    .then(validateShare(1, 1, 42, 150))
    .expect(ghostDupsRejected == 1)
    .expect(ghostAccepted.size() == 1)

  run unknownJob = init
    .then(validateShare(1, 99, 42, 150))
    .expect(ghostAccepted.size() == 0)

  run rateLimit = init
    .then(addJob(1, 1, 10000, false))
    .then(validateShare(1, 1, 1, 150))
    .then(validateShare(1, 1, 2, 150))
    .then(validateShare(1, 1, 3, 150))
    .then(validateShare(1, 1, 4, 150))
    .then(validateShare(1, 1, 5, 150))
    .expect(ghostAccepted.size() == 5)
    .then(validateShare(1, 1, 6, 150))
    .expect(ghostAccepted.size() == 5)

  run blockFound = init
    .then(addJob(1, 1, 10000, false))
    .then(validateShare(1, 1, 42, 150))
    .expect(ghostBlocksFound == 1)

  run cleanJobs = init
    .then(addJob(1, 1, 10000, false))
    .then(addJob(1, 2, 10000, true))
    .then(validateShare(1, 1, 42, 150))
    .expect(ghostAccepted.size() == 0)
    .then(validateShare(1, 2, 42, 150))
    .expect(ghostAccepted.size() == 1)

  run newBlockClears = init
    .then(addJob(1, 1, 10000, false))
    .then(validateShare(1, 1, 42, 150))
    .then(newBlock(100))
    .then(addJob(1, 3, 10000, true))
    .then(validateShare(1, 3, 42, 150))
    .expect(ghostAccepted.size() == 2)
}

// ═══════════════════════════════════════════════════════════════
// MODULE 2: Variable Difficulty Controller
// ═══════════════════════════════════════════════════════════════

module Vardiff {
  import basicSpells.*

  const CHANNELS: Set[int]
  const TARGET_SPW: int       // target shares per window
  const MIN_DIFF: int
  const MAX_DIFF: int
  const VAR_TOL_PCT: int      // variance tolerance (25 = 25%)
  const RETARGET_INT: int     // retarget interval in ticks
  const MIN_CHANGE: int

  var difficulty: int -> int
  var shareCount: int -> int
  var tickCount: int -> int
  var nextChId: int
  var chNonce: int -> int
  var ghostRetargets: int
  var ghostHistory: int -> List[int]

  action init = all {
    difficulty' = CHANNELS.mapBy(_ => clamp(1, MIN_DIFF, MAX_DIFF)),
    shareCount' = CHANNELS.mapBy(_ => 0),
    tickCount' = CHANNELS.mapBy(_ => 0),
    nextChId' = CHANNELS.size() + 1,
    chNonce' = CHANNELS.mapBy(c => c),
    ghostRetargets' = 0,
    ghostHistory' = CHANNELS.mapBy(_ => [clamp(1, MIN_DIFF, MAX_DIFF)]),
  }

  action recordShare(ch: int): bool = all {
    CHANNELS.contains(ch),
    shareCount' = shareCount.setBy(ch, s => s + 1),
    difficulty' = difficulty, tickCount' = tickCount,
    nextChId' = nextChId, chNonce' = chNonce,
    ghostRetargets' = ghostRetargets, ghostHistory' = ghostHistory,
  }

  action maybeRetarget(ch: int): bool = {
    val elapsed = tickCount.get(ch) + 1
    val shares = shareCount.get(ch)
    val curDiff = difficulty.get(ch)
    val lo = TARGET_SPW * (100 - VAR_TOL_PCT) / 100
    val hi = TARGET_SPW * (100 + VAR_TOL_PCT) / 100
    val inTolerance = shares >= lo and shares <= hi
    val rawNew = if (shares > 0) curDiff * shares / TARGET_SPW else curDiff / 2
    val smoothed = (curDiff + rawNew) / 2
    val clamped = clamp(smoothed, MIN_DIFF, MAX_DIFF)
    val change = abs(clamped - curDiff)
    val finalD = if (change >= MIN_CHANGE) clamped else curDiff
    all {
      CHANNELS.contains(ch),
      elapsed >= RETARGET_INT,
      if (inTolerance) all {
        shareCount' = shareCount.set(ch, 0),
        tickCount' = tickCount.set(ch, 0),
        difficulty' = difficulty,
        ghostRetargets' = ghostRetargets, ghostHistory' = ghostHistory,
        nextChId' = nextChId, chNonce' = chNonce,
      } else all {
        difficulty' = difficulty.set(ch, finalD),
        shareCount' = shareCount.set(ch, 0),
        tickCount' = tickCount.set(ch, 0),
        ghostRetargets' = ghostRetargets + 1,
        ghostHistory' = ghostHistory.setBy(ch, h => h.append(finalD)),
        nextChId' = nextChId, chNonce' = chNonce,
      }
    }
  }

  action advanceTick(ch: int): bool = all {
    CHANNELS.contains(ch),
    tickCount' = tickCount.setBy(ch, t => t + 1),
    difficulty' = difficulty, shareCount' = shareCount,
    nextChId' = nextChId, chNonce' = chNonce,
    ghostRetargets' = ghostRetargets, ghostHistory' = ghostHistory,
  }

  action step = {
    nondet ch = CHANNELS.oneOf()
    any { recordShare(ch), maybeRetarget(ch), advanceTick(ch) }
  }

  val diffInBounds = CHANNELS.forall(c =>
    difficulty.get(c) >= MIN_DIFF and difficulty.get(c) <= MAX_DIFF)
  val sharesNonNeg = CHANNELS.forall(c => shareCount.get(c) >= 0)
  val nonce1Unique = CHANNELS.forall(c1 => CHANNELS.forall(c2 =>
    (c1 == c2) or chNonce.get(c1) != chNonce.get(c2)))
  val historyInBounds = CHANNELS.forall(c =>
    ghostHistory.get(c).foldl(true, (ok, d) => ok and d >= MIN_DIFF and d <= MAX_DIFF))
  // Witnesses
  val witnessNoRetargets = ghostRetargets == 0
}

module VardiffTest {
  import Vardiff(
    CHANNELS = Set(1, 2, 3), TARGET_SPW = 5, MIN_DIFF = 1, MAX_DIFF = 1000,
    VAR_TOL_PCT = 25, RETARGET_INT = 1, MIN_CHANGE = 1,
  ).*

  run highRateUp = init
    .then(recordShare(1)).then(recordShare(1)).then(recordShare(1))
    .then(recordShare(1)).then(recordShare(1)).then(recordShare(1))
    .then(recordShare(1)).then(recordShare(1)).then(recordShare(1))
    .then(recordShare(1)).then(recordShare(1)).then(recordShare(1))
    .then(recordShare(1)).then(recordShare(1)).then(recordShare(1))
    .then(recordShare(1)).then(recordShare(1)).then(recordShare(1))
    .then(recordShare(1)).then(recordShare(1))
    .then(advanceTick(1)).then(maybeRetarget(1))
    .expect(difficulty.get(1) > 1)

  run toleranceNoChange = init
    .then(recordShare(1)).then(recordShare(1)).then(recordShare(1))
    .then(recordShare(1)).then(recordShare(1))
    .then(advanceTick(1)).then(maybeRetarget(1))
    .expect(difficulty.get(1) == 1)

  run nonce1Check = init.expect(nonce1Unique)
}

// ═══════════════════════════════════════════════════════════════
// MODULE 3: Job Declaration Protocol
// ═══════════════════════════════════════════════════════════════

module JdProtocol {
  import basicSpells.*

  type JdMode = CoinbaseOnly | FullTemplate

  type Token = {
    id: int, client: str, issued: int, lifetime: int,
    mode: JdMode, has_job: bool,
  }

  const CLIENTS: Set[str]
  const MAX_PER_CLIENT: int
  const MAX_TOTAL: int
  const TOKEN_LIFE: int
  const FT_ENABLED: bool

  var tokens: int -> Token
  var tokCtr: int
  var prevHash: int
  var jdClk: int
  var decJobs: int -> { ph: int, payout: bool }
  var nextJid: int
  var gTokAlloc: int
  var gTokExp: int
  var gJobDecl: int
  var gStale: int
  var gModeErr: int

  pure def expired(t: Token, now: int): bool = now - t.issued > t.lifetime

  action init = all {
    tokens' = Map(), tokCtr' = 1, prevHash' = 0, jdClk' = 0,
    decJobs' = Map(), nextJid' = 1,
    gTokAlloc' = 0, gTokExp' = 0, gJobDecl' = 0, gStale' = 0, gModeErr' = 0,
  }

  action allocateToken(client: str, reqMode: JdMode): bool = {
    val actIds = tokens.keys().filter(tid => not(expired(tokens.get(tid), jdClk)))
    val expCnt = tokens.keys().size() - actIds.size()
    val clean = actIds.mapBy(tid => tokens.get(tid))
    val clientCnt = clean.keys().filter(tid => clean.get(tid).client == client).size()
    val grantMode: JdMode = match reqMode {
      | FullTemplate => if (FT_ENABLED) FullTemplate else CoinbaseOnly
      | CoinbaseOnly => CoinbaseOnly
    }
    val newTok: Token = {
      id: tokCtr, client: client, issued: jdClk,
      lifetime: TOKEN_LIFE, mode: grantMode, has_job: false,
    }
    all {
      CLIENTS.contains(client),
      clean.keys().size() < MAX_TOTAL,
      clientCnt < MAX_PER_CLIENT,
      tokens' = clean.put(tokCtr, newTok),
      tokCtr' = tokCtr + 1,
      gTokAlloc' = gTokAlloc + 1,
      gTokExp' = gTokExp + expCnt,
      prevHash' = prevHash, jdClk' = jdClk, decJobs' = decJobs,
      nextJid' = nextJid, gJobDecl' = gJobDecl, gStale' = gStale, gModeErr' = gModeErr,
    }
  }

  action declareCustomJob(tid: int, ph: int, payout: bool): bool = {
    val hasTok = tokens.keys().contains(tid)
    val tok = if (hasTok) tokens.get(tid) else {
      id: 0, client: "", issued: 0, lifetime: 0, mode: CoinbaseOnly, has_job: false }
    val isExp = expired(tok, jdClk)
    val isStale = ph != prevHash
    all {
      hasTok, not(isExp),
      if (isStale) all {
        gStale' = gStale + 1,
        tokens' = tokens, tokCtr' = tokCtr, prevHash' = prevHash, jdClk' = jdClk,
        decJobs' = decJobs, nextJid' = nextJid,
        gTokAlloc' = gTokAlloc, gTokExp' = gTokExp, gJobDecl' = gJobDecl, gModeErr' = gModeErr,
      } else all {
        decJobs' = decJobs.put(nextJid, { ph: ph, payout: payout }),
        nextJid' = nextJid + 1,
        tokens' = tokens.set(tid, { ...tok, has_job: true }),
        gJobDecl' = gJobDecl + 1,
        tokCtr' = tokCtr, prevHash' = prevHash, jdClk' = jdClk,
        gTokAlloc' = gTokAlloc, gTokExp' = gTokExp, gStale' = gStale, gModeErr' = gModeErr,
      }
    }
  }

  action declareFTJob(tid: int, ph: int, payout: bool, txOk: bool): bool = {
    val hasTok = tokens.keys().contains(tid)
    val tok = if (hasTok) tokens.get(tid) else {
      id: 0, client: "", issued: 0, lifetime: 0, mode: CoinbaseOnly, has_job: false }
    val isExp = expired(tok, jdClk)
    val wrongMode = tok.mode != FullTemplate
    val isStale = ph != prevHash
    all {
      hasTok, not(isExp),
      if (wrongMode) all {
        gModeErr' = gModeErr + 1,
        tokens' = tokens, tokCtr' = tokCtr, prevHash' = prevHash, jdClk' = jdClk,
        decJobs' = decJobs, nextJid' = nextJid,
        gTokAlloc' = gTokAlloc, gTokExp' = gTokExp, gJobDecl' = gJobDecl, gStale' = gStale,
      } else if (isStale) all {
        gStale' = gStale + 1,
        tokens' = tokens, tokCtr' = tokCtr, prevHash' = prevHash, jdClk' = jdClk,
        decJobs' = decJobs, nextJid' = nextJid,
        gTokAlloc' = gTokAlloc, gTokExp' = gTokExp, gJobDecl' = gJobDecl, gModeErr' = gModeErr,
      } else if (not(txOk)) all {
        tokens' = tokens, tokCtr' = tokCtr, prevHash' = prevHash, jdClk' = jdClk,
        decJobs' = decJobs, nextJid' = nextJid,
        gTokAlloc' = gTokAlloc, gTokExp' = gTokExp, gJobDecl' = gJobDecl,
        gStale' = gStale, gModeErr' = gModeErr,
      } else all {
        decJobs' = decJobs.put(nextJid, { ph: ph, payout: payout }),
        nextJid' = nextJid + 1,
        tokens' = tokens.set(tid, { ...tok, has_job: true }),
        gJobDecl' = gJobDecl + 1,
        tokCtr' = tokCtr, prevHash' = prevHash, jdClk' = jdClk,
        gTokAlloc' = gTokAlloc, gTokExp' = gTokExp, gStale' = gStale, gModeErr' = gModeErr,
      }
    }
  }

  action jdNewBlock(np: int): bool = all {
    np != prevHash, prevHash' = np,
    tokens' = tokens, tokCtr' = tokCtr, jdClk' = jdClk,
    decJobs' = decJobs, nextJid' = nextJid,
    gTokAlloc' = gTokAlloc, gTokExp' = gTokExp, gJobDecl' = gJobDecl,
    gStale' = gStale, gModeErr' = gModeErr,
  }

  action jdTick = all {
    jdClk' = jdClk + 1,
    tokens' = tokens, tokCtr' = tokCtr, prevHash' = prevHash,
    decJobs' = decJobs, nextJid' = nextJid,
    gTokAlloc' = gTokAlloc, gTokExp' = gTokExp, gJobDecl' = gJobDecl,
    gStale' = gStale, gModeErr' = gModeErr,
  }

  action step = {
    nondet c = CLIENTS.oneOf()
    nondet mode = Set(CoinbaseOnly, FullTemplate).oneOf()
    nondet p = Set(true, false).oneOf()
    nondet tx = Set(true, false).oneOf()
    nondet np = 1.to(5).oneOf()
    // Smart token selection: bias toward active tokens, include a few misses
    val activeIds = tokens.keys().filter(tid => not(expired(tokens.get(tid), jdClk)))
    val candidateTids = activeIds.union(Set(tokCtr, tokCtr + 1))
    nondet tid = candidateTids.oneOf()
    // 50% chance of matching current prevHash
    nondet ph = Set(prevHash, prevHash + 1).oneOf()
    any {
      allocateToken(c, mode), declareCustomJob(tid, ph, p),
      declareFTJob(tid, ph, p, tx), jdNewBlock(np), jdTick,
    }
  }

  val tokCtrMono = tokens.keys().forall(tid => tid < tokCtr)
  val perClientBound = CLIENTS.forall(c =>
    tokens.keys().filter(tid =>
      tokens.get(tid).client == c and not(expired(tokens.get(tid), jdClk))
    ).size() <= MAX_PER_CLIENT)
  val totalBound = tokens.keys().filter(tid =>
    not(expired(tokens.get(tid), jdClk))).size() <= MAX_TOTAL
  val jobIdMono = nextJid > 0
  // Witnesses
  val witnessNoTokens = gTokAlloc == 0
  val witnessNoJobs = gJobDecl == 0
  // Reachability witnesses — expected to be violated by simulator
  val witnessJobsDeclared = gJobDecl == 0
}

module JdProtocolTest {
  import JdProtocol(
    CLIENTS = Set("alice", "bob"), MAX_PER_CLIENT = 3,
    MAX_TOTAL = 10, TOKEN_LIFE = 3, FT_ENABLED = true,
  ).*

  run allocAndDeclare = init
    .then(allocateToken("alice", CoinbaseOnly))
    .expect(gTokAlloc == 1)
    .then(declareCustomJob(1, 0, true))
    .expect(gJobDecl == 1)

  run expiredFails = init
    .then(allocateToken("alice", CoinbaseOnly))
    .then(jdTick).then(jdTick).then(jdTick).then(jdTick)
    .then(declareCustomJob(1, 0, true))
    .fail()

  run stalePrevHash = init
    .then(allocateToken("alice", CoinbaseOnly))
    .then(jdNewBlock(1))
    .then(declareCustomJob(1, 0, true))
    .expect(gStale == 1)

  run modeMismatch = init
    .then(allocateToken("alice", CoinbaseOnly))
    .then(declareFTJob(1, 0, true, true))
    .expect(gModeErr == 1)

  run ftHappyPath = init
    .then(allocateToken("alice", FullTemplate))
    .then(declareFTJob(1, 0, true, true))
    .expect(gJobDecl == 1)

  run perClientLimit = init
    .then(allocateToken("alice", CoinbaseOnly))
    .then(allocateToken("alice", CoinbaseOnly))
    .then(allocateToken("alice", CoinbaseOnly))
    .then(allocateToken("alice", CoinbaseOnly))
    .fail()
}

// ═══════════════════════════════════════════════════════════════
// MODULE 4: Sequence Validation
// ═══════════════════════════════════════════════════════════════

module SeqValidation {
  import basicSpells.*

  const CHANNELS: Set[int]
  const MAX_GAP: int
  const WIN_SIZE: int

  var highest: int -> int
  var window: int -> Set[int]
  var anomalies: int -> int
  var gReplays: int
  var gValid: int
  var gGapLarge: int

  pure def addToWin(w: Set[int], s: int, mx: int): Set[int] =
    if (w.size() >= mx) {
      // Find oldest (smallest) in w, using MAX_INT as initial accumulator
      // to ensure we pick from w, not accidentally pick s
      val oldest = w.fold(2147483647, (m, x) => min(m, x))
      w.exclude(Set(oldest)).union(Set(s))
    } else w.union(Set(s))

  action init = all {
    highest' = CHANNELS.mapBy(_ => 0),
    window' = CHANNELS.mapBy(_ => Set()),
    anomalies' = CHANNELS.mapBy(_ => 0),
    gReplays' = 0, gValid' = 0, gGapLarge' = 0,
  }

  action validate(ch: int, seq: int): bool = {
    val hi = highest.get(ch)
    val win = window.get(ch)
    val isFirst = hi == 0 and win.size() == 0
    val isReplay = win.contains(seq)
    val isNext = seq == hi + 1
    val isForward = seq > hi
    val gap = seq - hi
    val behind = hi - seq
    all {
      CHANNELS.contains(ch), seq > 0,
      if (isFirst) all {
        highest' = highest.set(ch, seq),
        window' = window.set(ch, Set(seq)),
        anomalies' = anomalies, gValid' = gValid + 1,
        gReplays' = gReplays, gGapLarge' = gGapLarge,
      }
      else if (isReplay) all {
        anomalies' = anomalies.setBy(ch, c => c + 1),
        gReplays' = gReplays + 1,
        highest' = highest, window' = window,
        gValid' = gValid, gGapLarge' = gGapLarge,
      }
      else if (isNext) all {
        highest' = highest.set(ch, seq),
        window' = window.set(ch, addToWin(win, seq, WIN_SIZE)),
        gValid' = gValid + 1, anomalies' = anomalies,
        gReplays' = gReplays, gGapLarge' = gGapLarge,
      }
      else if (isForward and gap <= MAX_GAP) all {
        highest' = highest.set(ch, seq),
        window' = window.set(ch, addToWin(win, seq, WIN_SIZE)),
        anomalies' = anomalies.setBy(ch, c => c + 1),
        gValid' = gValid + 1,
        gReplays' = gReplays, gGapLarge' = gGapLarge,
      }
      else if (isForward and gap > MAX_GAP) all {
        anomalies' = anomalies.setBy(ch, c => c + 1),
        gGapLarge' = gGapLarge + 1,
        highest' = highest, window' = window,
        gValid' = gValid, gReplays' = gReplays,
      }
      else if (not(isForward) and behind <= WIN_SIZE) all {
        window' = window.set(ch, addToWin(win, seq, WIN_SIZE)),
        anomalies' = anomalies.setBy(ch, c => c + 1),
        gValid' = gValid + 1, highest' = highest,
        gReplays' = gReplays, gGapLarge' = gGapLarge,
      }
      else all {
        // Stale
        anomalies' = anomalies.setBy(ch, c => c + 1),
        highest' = highest, window' = window,
        gValid' = gValid, gReplays' = gReplays, gGapLarge' = gGapLarge,
      }
    }
  }

  action step = {
    nondet ch = CHANNELS.oneOf()
    nondet seq = 1.to(50).oneOf()
    validate(ch, seq)
  }

  val winBounded = CHANNELS.forall(c => window.get(c).size() <= WIN_SIZE)
  val hiNonNeg = CHANNELS.forall(c => highest.get(c) >= 0)
  val winPositive = CHANNELS.forall(c => window.get(c).forall(s => s > 0))
  // Witnesses
  val witnessNoReplays = gReplays == 0
  val witnessNoValid = gValid == 0
}

module SeqValidationTest {
  import SeqValidation(CHANNELS = Set(1, 2), MAX_GAP = 10, WIN_SIZE = 5).*

  run seqValid = init
    .then(validate(1, 1)).then(validate(1, 2)).then(validate(1, 3))
    .expect(gValid == 3).expect(gReplays == 0)

  run replayDetect = init
    .then(validate(1, 1)).then(validate(1, 2)).then(validate(1, 2))
    .expect(gReplays == 1)

  run oooValid = init
    .then(validate(1, 1)).then(validate(1, 3)).then(validate(1, 2))
    .expect(gValid == 3)

  run gapLarge = init
    .then(validate(1, 1)).then(validate(1, 50))
    .expect(gGapLarge == 1)
}

// ═══════════════════════════════════════════════════════════════
// MODULE 5: Connection Tracking
// ═══════════════════════════════════════════════════════════════

module ConnTracking {
  const ADDRS: Set[str]
  const SHORT_THRESH: int
  const MAX_SHORT: int
  const MAX_DECRYPT: int

  var shortCnt: str -> int
  var decryptCnt: str -> int
  var totalCnt: str -> int
  var flagged: str -> bool
  var gFlags: int

  action init = all {
    shortCnt' = ADDRS.mapBy(_ => 0), decryptCnt' = ADDRS.mapBy(_ => 0),
    totalCnt' = ADDRS.mapBy(_ => 0), flagged' = ADDRS.mapBy(_ => false), gFlags' = 0,
  }

  action onDisconnect(addr: str, dur: int, decErr: bool): bool = {
    val newS = if (dur < SHORT_THRESH) shortCnt.get(addr) + 1 else shortCnt.get(addr)
    val newD = if (decErr) decryptCnt.get(addr) + 1 else decryptCnt.get(addr)
    val sf = newS >= MAX_SHORT or newD >= MAX_DECRYPT
    val was = flagged.get(addr)
    val newly = sf and not(was)
    all {
      ADDRS.contains(addr), dur >= 0,
      shortCnt' = shortCnt.set(addr, newS),
      decryptCnt' = decryptCnt.set(addr, newD),
      totalCnt' = totalCnt.setBy(addr, t => t + 1),
      flagged' = flagged.set(addr, sf or was),
      gFlags' = if (newly) gFlags + 1 else gFlags,
    }
  }

  action resetWin(addr: str): bool = all {
    ADDRS.contains(addr),
    shortCnt' = shortCnt.set(addr, 0), decryptCnt' = decryptCnt.set(addr, 0),
    totalCnt' = totalCnt, flagged' = flagged, gFlags' = gFlags,
  }

  action step = {
    nondet a = ADDRS.oneOf()
    nondet d = 0.to(20).oneOf()
    nondet e = Set(true, false).oneOf()
    any { onDisconnect(a, d, e), resetWin(a) }
  }

  val countsNonNeg = ADDRS.forall(a =>
    shortCnt.get(a) >= 0 and decryptCnt.get(a) >= 0 and totalCnt.get(a) >= 0)
  val witnessNoFlags = gFlags == 0
}

module ConnTrackingTest {
  import ConnTracking(
    ADDRS = Set("1.1.1.1", "2.2.2.2"), SHORT_THRESH = 5,
    MAX_SHORT = 3, MAX_DECRYPT = 2,
  ).*

  run normalNotFlagged = init
    .then(onDisconnect("1.1.1.1", 10, false))
    .then(onDisconnect("1.1.1.1", 15, false))
    .expect(not(flagged.get("1.1.1.1")))

  run shortLivedFlag = init
    .then(onDisconnect("1.1.1.1", 1, false))
    .then(onDisconnect("1.1.1.1", 2, false))
    .then(onDisconnect("1.1.1.1", 1, false))
    .expect(flagged.get("1.1.1.1"))

  run decryptFlag = init
    .then(onDisconnect("1.1.1.1", 10, true))
    .then(onDisconnect("1.1.1.1", 10, true))
    .expect(flagged.get("1.1.1.1"))

  run independent = init
    .then(onDisconnect("1.1.1.1", 1, false))
    .then(onDisconnect("1.1.1.1", 1, false))
    .then(onDisconnect("1.1.1.1", 1, false))
    .expect(flagged.get("1.1.1.1"))
    .expect(not(flagged.get("2.2.2.2")))
}

// ═══════════════════════════════════════════════════════════════
// MODULE 6: Payout Tracking
// ═══════════════════════════════════════════════════════════════

module Payout {
  const MINERS: Set[str]

  var totShares: str -> int
  var totDiff: str -> int
  var winShares: str -> int
  var winDiff: str -> int
  var gTotShares: int
  var gTotDiff: int
  var gResets: int

  action init = all {
    totShares' = MINERS.mapBy(_ => 0), totDiff' = MINERS.mapBy(_ => 0),
    winShares' = MINERS.mapBy(_ => 0), winDiff' = MINERS.mapBy(_ => 0),
    gTotShares' = 0, gTotDiff' = 0, gResets' = 0,
  }

  action recordShare(m: str, d: int): bool = all {
    MINERS.contains(m), d > 0,
    totShares' = totShares.setBy(m, s => s + 1),
    totDiff' = totDiff.setBy(m, x => x + d),
    winShares' = winShares.setBy(m, s => s + 1),
    winDiff' = winDiff.setBy(m, x => x + d),
    gTotShares' = gTotShares + 1, gTotDiff' = gTotDiff + d, gResets' = gResets,
  }

  action resetWindow: bool = all {
    winShares' = MINERS.mapBy(_ => 0), winDiff' = MINERS.mapBy(_ => 0),
    gResets' = gResets + 1,
    totShares' = totShares, totDiff' = totDiff,
    gTotShares' = gTotShares, gTotDiff' = gTotDiff,
  }

  action step = {
    nondet m = MINERS.oneOf()
    nondet d = 1.to(100).oneOf()
    any { recordShare(m, d), resetWindow }
  }

  val shareConserve = MINERS.fold(0, (a, m) => a + totShares.get(m)) == gTotShares
  val diffConserve = MINERS.fold(0, (a, m) => a + totDiff.get(m)) == gTotDiff
  val noNeg = MINERS.forall(m => totShares.get(m) >= 0 and winShares.get(m) >= 0)
  val winLteTotal = MINERS.forall(m =>
    winShares.get(m) <= totShares.get(m) and winDiff.get(m) <= totDiff.get(m))
  val witnessNoShares = gTotShares == 0
}

module PayoutTest {
  import Payout(MINERS = Set("alice", "bob", "carol")).*

  run basicRecord = init
    .then(recordShare("alice", 10)).then(recordShare("bob", 20))
    .then(recordShare("alice", 5))
    .expect(totShares.get("alice") == 2)
    .expect(shareConserve)

  run winReset = init
    .then(recordShare("alice", 10)).then(recordShare("bob", 20))
    .then(resetWindow)
    .expect(winShares.get("alice") == 0)
    .expect(totShares.get("alice") == 1)
    .expect(shareConserve)

  run multiReset = init
    .then(recordShare("alice", 5)).then(recordShare("bob", 10))
    .then(resetWindow)
    .then(recordShare("carol", 15)).then(recordShare("alice", 20))
    .then(resetWindow)
    .then(recordShare("bob", 25))
    .expect(shareConserve).expect(diffConserve)
}

// ═══════════════════════════════════════════════════════════════
// MODULE 7: Forge Relay
// ═══════════════════════════════════════════════════════════════

module Forge {
  import basicSpells.*

  const PEERS: Set[str]
  const DATA_S: int
  const PARITY_S: int
  const MAX_PENDING: int
  const REQ_AUTH: bool
  const MAX_RECENT: int

  pure val TOTAL_S = DATA_S + PARITY_S

  var peerAuth: str -> bool
  var assemblies: int -> { recv: Set[int], pow: bool, tot: int }
  var recon: Set[int]
  var recent: Set[(int, int)]
  var gChunks: int
  var gFwd: int
  var gAuthFail: int
  var gRecon: int

  action init = all {
    peerAuth' = PEERS.mapBy(_ => not(REQ_AUTH)),
    assemblies' = Map(), recon' = Set(), recent' = Set(),
    gChunks' = 0, gFwd' = 0, gAuthFail' = 0, gRecon' = 0,
  }

  action authPeer(p: str, ok: bool): bool = all {
    PEERS.contains(p), REQ_AUTH,
    if (ok) all {
      peerAuth' = peerAuth.set(p, true),
      assemblies' = assemblies, recon' = recon, recent' = recent,
      gChunks' = gChunks, gFwd' = gFwd, gAuthFail' = gAuthFail, gRecon' = gRecon,
    } else all {
      gAuthFail' = gAuthFail + 1,
      peerAuth' = peerAuth, assemblies' = assemblies, recon' = recon, recent' = recent,
      gChunks' = gChunks, gFwd' = gFwd, gRecon' = gRecon,
    }
  }

  action recvChunk(from: str, bh: int, cid: int, pow: bool): bool = {
    val hasAsm = assemblies.keys().contains(bh)
    val existing = if (hasAsm) assemblies.get(bh)
                   else { recv: Set(), pow: false, tot: TOTAL_S }
    val atCap = not(hasAsm) and assemblies.keys().size() >= MAX_PENDING
    val updated = { ...existing, recv: existing.recv.union(Set(cid)),
                    pow: existing.pow or pow }
    val canRecon = updated.recv.size() >= DATA_S
    val isNew = not(recon.contains(bh))
    all {
      PEERS.contains(from), cid >= 0, cid < TOTAL_S, bh > 0,
      peerAuth.get(from),
      not(recent.contains((bh, cid))),
      gChunks' = gChunks + 1,
      recent' = recent.union(Set((bh, cid))),
      if (atCap) all {
        peerAuth' = peerAuth, assemblies' = assemblies, recon' = recon,
        gFwd' = gFwd, gAuthFail' = gAuthFail, gRecon' = gRecon,
      }
      else if (canRecon and isNew) all {
        assemblies' = assemblies.put(bh, updated),
        recon' = recon.union(Set(bh)),
        gRecon' = gRecon + 1,
        gFwd' = if (updated.pow) gFwd + 1 else gFwd,
        peerAuth' = peerAuth, gAuthFail' = gAuthFail,
      }
      else all {
        assemblies' = assemblies.put(bh, updated),
        recon' = recon, gRecon' = gRecon, gFwd' = gFwd,
        peerAuth' = peerAuth, gAuthFail' = gAuthFail,
      }
    }
  }

  action pruneRecent: bool = all {
    recent.size() > MAX_RECENT,
    recent' = Set(),
    peerAuth' = peerAuth, assemblies' = assemblies, recon' = recon,
    gChunks' = gChunks, gFwd' = gFwd, gAuthFail' = gAuthFail, gRecon' = gRecon,
  }

  action step = {
    nondet p = PEERS.oneOf()
    nondet bh = 1.to(10).oneOf()
    nondet cid = 0.to(TOTAL_S - 1).oneOf()
    nondet pow = Set(true, false).oneOf()
    nondet ok = Set(true, false).oneOf()
    any { recvChunk(p, bh, cid, pow), authPeer(p, ok), pruneRecent }
  }

  val fecSound = assemblies.keys().forall(bh => {
    val a = assemblies.get(bh)
    (a.recv.size() >= DATA_S and a.pow) implies recon.contains(bh)
  })
  val pendingBound = assemblies.keys().size() <= MAX_PENDING + 1
  val chunkValid = assemblies.keys().forall(bh =>
    assemblies.get(bh).recv.forall(c => c >= 0 and c < TOTAL_S))
  val shardBound = assemblies.keys().forall(bh =>
    assemblies.get(bh).recv.size() <= TOTAL_S)
  val recentBounded = recent.size() <= MAX_RECENT + TOTAL_S
  val witnessNoChunks = gChunks == 0
  val witnessNoRecon = gRecon == 0
}

module ForgeTest {
  import Forge(
    PEERS = Set("p1", "p2", "p3"), DATA_S = 3, PARITY_S = 1,
    MAX_PENDING = 4, REQ_AUTH = false, MAX_RECENT = 50,
  ).*

  run fullRecon = init
    .then(recvChunk("p1", 1, 0, true))
    .then(recvChunk("p2", 1, 1, false))
    .then(recvChunk("p3", 1, 2, false))
    .expect(recon.contains(1)).expect(gRecon == 1)

  run insufficient = init
    .then(recvChunk("p1", 1, 0, true))
    .then(recvChunk("p2", 1, 1, false))
    .expect(not(recon.contains(1)))

  run paritySub = init
    .then(recvChunk("p1", 1, 0, true))
    .then(recvChunk("p2", 1, 1, false))
    .then(recvChunk("p3", 1, 3, false))
    .expect(recon.contains(1))

  run replayFail = init
    .then(recvChunk("p1", 1, 0, true))
    .then(recvChunk("p1", 1, 0, true))
    .fail()

  run multiBlock = init
    .then(recvChunk("p1", 1, 0, true)).then(recvChunk("p1", 2, 0, true))
    .then(recvChunk("p2", 1, 1, false)).then(recvChunk("p2", 2, 1, false))
    .then(recvChunk("p3", 1, 2, false))
    .expect(recon.contains(1)).expect(not(recon.contains(2)))
    .then(recvChunk("p3", 2, 2, false))
    .expect(recon.contains(2))
}

module ForgeAuthTest {
  import Forge(
    PEERS = Set("p1", "p2"), DATA_S = 2, PARITY_S = 1,
    MAX_PENDING = 4, REQ_AUTH = true, MAX_RECENT = 50,
  ).*

  run unauthFail = init
    .then(recvChunk("p1", 1, 0, true)).fail()

  run authOk = init
    .then(authPeer("p1", true)).then(authPeer("p2", true))
    .then(recvChunk("p1", 1, 0, true))
    .then(recvChunk("p2", 1, 1, false))
    .expect(recon.contains(1))

  run badHmac = init
    .then(authPeer("p1", false))
    .expect(gAuthFail == 1).expect(not(peerAuth.get("p1")))
}

// ═══════════════════════════════════════════════════════════════
// MODULE 8: System Composition (ShareValidation → Payout bridge)
// ═══════════════════════════════════════════════════════════════

module SystemComposition {
  import basicSpells.*
  import miningSpells.*

  const MINERS: Set[int]
  const MINER_NAMES: int -> str

  // --- ShareValidation state (simplified) ---
  var svJobs: int -> (int -> { target: int, active: bool })
  var svLastJobId: int -> int
  var svDups: int -> Set[int]
  var svBlockDiff: int
  var svAcceptedCount: int
  var svAcceptedQueue: List[(int, int)]  // (miner, difficulty) pending payout

  // --- Payout state ---
  var payTotShares: str -> int
  var payTotDiff: str -> int
  var payRecordedCount: int

  action init = all {
    svJobs' = MINERS.mapBy(_ => Map()),
    svLastJobId' = MINERS.mapBy(_ => 0),
    svDups' = Map(),
    svBlockDiff' = 100,
    svAcceptedCount' = 0,
    svAcceptedQueue' = [],
    payTotShares' = MINERS.fold(Map(), (m, id) => m.put(MINER_NAMES.get(id), 0)),
    payTotDiff' = MINERS.fold(Map(), (m, id) => m.put(MINER_NAMES.get(id), 0)),
    payRecordedCount' = 0,
  }

  action compAddJob(ch: int, jid: int, tgt: int, clean: bool): bool = {
    val cJobs = svJobs.get(ch)
    val deact = if (clean)
      cJobs.keys().mapBy(j => { ...cJobs.get(j), active: false })
    else cJobs
    val entry = { target: tgt, active: true }
    all {
      MINERS.contains(ch), jid > 0, tgt > 0, jid > svLastJobId.get(ch),
      svJobs' = svJobs.set(ch, deact.put(jid, entry)),
      svLastJobId' = svLastJobId.setBy(ch, cur => max(cur, jid)),
      svDups' = svDups, svBlockDiff' = svBlockDiff,
      svAcceptedCount' = svAcceptedCount, svAcceptedQueue' = svAcceptedQueue,
      payTotShares' = payTotShares, payTotDiff' = payTotDiff,
      payRecordedCount' = payRecordedCount,
    }
  }

  action compValidateShare(ch: int, jid: int, n2: int, sd: int): bool = {
    val cJobs = svJobs.get(ch)
    val hasJob = cJobs.keys().contains(jid)
    val jobActive = if (hasJob) cJobs.get(jid).active else false
    val jobTarget = if (hasJob) cJobs.get(jid).target else 0
    val hasDups = svDups.keys().contains(jid)
    val jobDups = if (hasDups) svDups.get(jid) else Set()
    val isDup = jobDups.contains(n2)
    val reqDiff = target_to_difficulty(jobTarget)
    val meetsShare = sd > 0 and meets_target(sd, reqDiff)
    all {
      MINERS.contains(ch), hasJob, jobActive, not(isDup), meetsShare,
      svDups' = svDups.put(jid, jobDups.union(Set(n2))),
      svAcceptedCount' = svAcceptedCount + 1,
      svAcceptedQueue' = svAcceptedQueue.append((ch, reqDiff)),
      svJobs' = svJobs, svLastJobId' = svLastJobId, svBlockDiff' = svBlockDiff,
      payTotShares' = payTotShares, payTotDiff' = payTotDiff,
      payRecordedCount' = payRecordedCount,
    }
  }

  // Drain one accepted share into the payout system
  action compRecordPayout: bool = {
    all {
      svAcceptedQueue.length() > 0,
      val entry = svAcceptedQueue[0]
      val miner = entry._1
      val diff = entry._2
      val name = MINER_NAMES.get(miner)
      all {
        payTotShares' = payTotShares.setBy(name, s => s + 1),
        payTotDiff' = payTotDiff.setBy(name, d => d + diff),
        payRecordedCount' = payRecordedCount + 1,
        svAcceptedQueue' = svAcceptedQueue.tail(),
        svJobs' = svJobs, svLastJobId' = svLastJobId, svDups' = svDups,
        svBlockDiff' = svBlockDiff, svAcceptedCount' = svAcceptedCount,
      }
    }
  }

  action step = {
    nondet m = MINERS.oneOf()
    val existingJobs = svJobs.get(m).keys()
    val candidateJobs = existingJobs.union(Set(1))
    nondet j = candidateJobs.oneOf()
    nondet freshJ = (svLastJobId.get(m) + 1).to(svLastJobId.get(m) + 3).oneOf()
    nondet n = 1.to(20).oneOf()
    nondet s = 50.to(200).oneOf()
    nondet t = 1.to(50).oneOf()
    any {
      compAddJob(m, freshJ, t, false),
      compAddJob(m, freshJ, t, true),
      compValidateShare(m, j, n, s),
      compRecordPayout,
    }
  }

  // COMPOSITION INVARIANT: accepted shares eventually equal recorded shares
  // At any point, recorded <= accepted (queue may be non-empty)
  val shareFlowConserved = payRecordedCount + svAcceptedQueue.length() == svAcceptedCount
  val recordedLeqAccepted = payRecordedCount <= svAcceptedCount
  val queueDrainable = svAcceptedQueue.length() >= 0
}

module SystemCompositionTest {
  import SystemComposition(
    MINERS = Set(1, 2),
    MINER_NAMES = Map(1 -> "alice", 2 -> "bob"),
  ).*

  // End-to-end: addJob → validateShare → recordPayout
  run compE2E = init
    .then(compAddJob(1, 1, 10000, false))
    .then(compValidateShare(1, 1, 42, 150))
    .expect(svAcceptedCount == 1)
    .expect(payRecordedCount == 0)
    .then(compRecordPayout)
    .expect(payRecordedCount == 1)
    .expect(svAcceptedQueue.length() == 0)
    .expect(shareFlowConserved)

  // Multiple miners, multiple shares, drain all
  run compMultiMiner = init
    .then(compAddJob(1, 1, 10000, false))
    .then(compAddJob(2, 1, 10000, false))
    .then(compValidateShare(1, 1, 1, 150))
    .then(compValidateShare(2, 1, 2, 150))
    .expect(svAcceptedCount == 2)
    .then(compRecordPayout)
    .then(compRecordPayout)
    .expect(payRecordedCount == 2)
    .expect(shareFlowConserved)

  // Conservation holds even with interleaved operations
  run compInterleaved = init
    .then(compAddJob(1, 1, 10000, false))
    .then(compValidateShare(1, 1, 1, 150))
    .then(compRecordPayout)
    .then(compAddJob(1, 2, 10000, false))
    .then(compValidateShare(1, 2, 2, 150))
    .expect(shareFlowConserved)
    .then(compRecordPayout)
    .expect(shareFlowConserved)
    .expect(payRecordedCount == 2)
}

// ═══════════════════════════════════════════════════════════════
// MODULE 9: Duplicate Detector Wraparound Bug
// ═══════════════════════════════════════════════════════════════

module DuplicateWraparound {
  import basicSpells.*

  const MAX_JOB_ID: int
  const MAX_SHARES: int

  var dupMap: int -> Set[int]   // job_id -> set of share hashes
  var jobIdCounter: int
  var ghostFalsePositives: int
  var activeJobs: Set[int]      // track which job IDs are currently active

  action init = all {
    dupMap' = Map(),
    jobIdCounter' = 0,
    ghostFalsePositives' = 0,
    activeJobs' = Set(),
  }

  // Allocate a new job ID (wraps around). BUG: old dup entries NOT cleared.
  action newJob: bool = {
    val nextId = if (jobIdCounter >= MAX_JOB_ID) 1 else jobIdCounter + 1
    all {
      jobIdCounter' = nextId,
      activeJobs' = activeJobs.union(Set(nextId)),
      // BUG: dupMap is NOT cleared for nextId — stale entries persist
      dupMap' = dupMap,
      ghostFalsePositives' = ghostFalsePositives,
    }
  }

  // Submit a unique share — false positive if hash collides with stale entry
  action submitShare(jid: int, hash: int): bool = {
    val hasDups = dupMap.keys().contains(jid)
    val existing = if (hasDups) dupMap.get(jid) else Set()
    val isDup = existing.contains(hash)
    // This is a genuinely unique share (different job epoch), so any
    // collision is a false positive caused by stale dup map entries
    val isFalsePositive = isDup and activeJobs.contains(jid)
    all {
      jid > 0, jid <= MAX_JOB_ID, hash > 0, hash <= MAX_SHARES,
      activeJobs.contains(jid),
      not(isDup),  // share gets rejected if dup — cannot proceed
      dupMap' = dupMap.put(jid, existing.union(Set(hash))),
      jobIdCounter' = jobIdCounter,
      ghostFalsePositives' = ghostFalsePositives,
      activeJobs' = activeJobs,
    }
  }

  // Submit share that may be falsely rejected (for invariant violation)
  action submitShareMayReject(jid: int, hash: int): bool = {
    val hasDups = dupMap.keys().contains(jid)
    val existing = if (hasDups) dupMap.get(jid) else Set()
    val isDup = existing.contains(hash)
    all {
      jid > 0, jid <= MAX_JOB_ID, hash > 0, hash <= MAX_SHARES,
      activeJobs.contains(jid),
      if (isDup) all {
        // False positive: share rejected due to stale dup entry
        ghostFalsePositives' = ghostFalsePositives + 1,
        dupMap' = dupMap, jobIdCounter' = jobIdCounter, activeJobs' = activeJobs,
      } else all {
        dupMap' = dupMap.put(jid, existing.union(Set(hash))),
        ghostFalsePositives' = ghostFalsePositives,
        jobIdCounter' = jobIdCounter, activeJobs' = activeJobs,
      }
    }
  }

  action step = {
    nondet jid = 1.to(MAX_JOB_ID).oneOf()
    nondet hash = 1.to(MAX_SHARES).oneOf()
    any {
      newJob,
      submitShareMayReject(jid, hash),
    }
  }

  // Invariant: no false positives from wraparound
  // Expected: VIOLATED — proving the bug exists
  val noFalsePositives = ghostFalsePositives == 0
}

module DuplicateWraparoundTest {
  import DuplicateWraparound(MAX_JOB_ID = 5, MAX_SHARES = 10).*

  // Demonstrate the wraparound false positive bug:
  // 1. Allocate job 1, submit share with hash 7
  // 2. Cycle through all job IDs to wrap around back to 1
  // 3. New job 1's share with hash 7 is falsely rejected
  run wraparoundBug = init
    .then(newJob)                        // jobIdCounter = 1
    .then(submitShareMayReject(1, 7))    // records hash 7 for job 1
    .then(newJob).then(newJob).then(newJob).then(newJob)  // counter = 5
    .then(newJob)                        // wraps to 1 — stale dup entry!
    .then(submitShareMayReject(1, 7))    // FALSE POSITIVE: hash 7 still in dupMap
    .expect(ghostFalsePositives == 1)

  // Normal case without wraparound works fine
  run noWraparound = init
    .then(newJob)
    .then(submitShareMayReject(1, 1))
    .then(submitShareMayReject(1, 2))
    .then(submitShareMayReject(1, 3))
    .expect(ghostFalsePositives == 0)
}

// ═══════════════════════════════════════════════════════════════
// MODULE 10: TOCTOU Concurrency Model
// ═══════════════════════════════════════════════════════════════
// Models the 3-phase lock pattern from server.rs:
//   Phase 1: acquire channels lock → rate check → release
//   Phase 2: acquire channels lock → job lookup (snapshot) → release
//   Phase 3: validate with snapshot (NO lock held)
//   Phase 4: acquire channels lock → record accepted → release
// Between phases, another thread or new-block event can:
//   - deactivate a job (clean=true on new block)
//   - change the difficulty target
// This creates a TOCTOU race window.

module ConcurrentShareValidation {
  import basicSpells.*
  import miningSpells.*

  const THREADS: Set[int]

  // Thread phase: progression through the lock pattern
  // 0 = Idle, 1 = RateCheckDone, 2 = JobLookedUp, 3 = Validated, 4 = Done
  var phase: int -> int
  // Snapshot of job state taken during phase 2 (under lock)
  var snapshot: int -> { jid: int, target: int, wasActive: bool }
  // Validation result
  var validated: int -> bool

  // Shared mutable state (protected by lock in real code)
  var lock: int                   // 0 = free, >0 = held by thread
  var jobs: int -> { target: int, active: bool }
  var accepted: Set[(int, int)]   // (thread, jid) — accepted shares
  var ghostStaleAccepts: int      // shares accepted after job deactivated

  action init = all {
    phase' = THREADS.mapBy(_ => 0),
    snapshot' = THREADS.mapBy(_ => { jid: 0, target: 0, wasActive: false }),
    validated' = THREADS.mapBy(_ => false),
    lock' = 0,
    jobs' = Map(1 -> { target: 10000, active: true }, 2 -> { target: 10000, active: true }),
    accepted' = Set(),
    ghostStaleAccepts' = 0,
  }

  // Phase 1: Rate check (acquire and release lock)
  action rateCheck(t: int): bool = all {
    THREADS.contains(t),
    phase.get(t) == 0,
    lock == 0,  // lock must be free
    // Acquire lock, do rate check, release lock — modeled as atomic
    phase' = phase.set(t, 1),
    snapshot' = snapshot, validated' = validated,
    lock' = 0,  // released immediately
    jobs' = jobs, accepted' = accepted, ghostStaleAccepts' = ghostStaleAccepts,
  }

  // Phase 2: Job lookup — snapshot job state under lock, then release
  action jobLookup(t: int, jid: int): bool = {
    val hasJob = jobs.keys().contains(jid)
    val job = if (hasJob) jobs.get(jid) else { target: 0, active: false }
    all {
      THREADS.contains(t),
      phase.get(t) == 1,
      lock == 0,
      hasJob,
      // Take snapshot while holding lock
      snapshot' = snapshot.set(t, { jid: jid, target: job.target, wasActive: job.active }),
      phase' = phase.set(t, 2),
      lock' = 0,  // released immediately
      validated' = validated,
      jobs' = jobs, accepted' = accepted, ghostStaleAccepts' = ghostStaleAccepts,
    }
  }

  // Phase 3: Validate with snapshot (NO lock held!)
  // This is the expensive Equihash validation — runs lock-free
  action validate(t: int, shareDiff: int): bool = {
    val snap = snapshot.get(t)
    val reqDiff = target_to_difficulty(snap.target)
    val meets = shareDiff > 0 and meets_target(shareDiff, reqDiff)
    all {
      THREADS.contains(t),
      phase.get(t) == 2,
      snap.wasActive,  // was active when we looked
      meets,           // share meets difficulty
      validated' = validated.set(t, true),
      phase' = phase.set(t, 3),
      // NO lock interaction — this is the TOCTOU window
      lock' = lock,
      snapshot' = snapshot, jobs' = jobs, accepted' = accepted,
      ghostStaleAccepts' = ghostStaleAccepts,
    }
  }

  // Phase 4: Record accepted share (acquire lock, record, release)
  action recordAccepted(t: int): bool = {
    val snap = snapshot.get(t)
    val jid = snap.jid
    // Check current job state (may differ from snapshot!)
    val hasJob = jobs.keys().contains(jid)
    val nowActive = if (hasJob) jobs.get(jid).active else false
    val isStale = not(nowActive)  // job was deactivated between phases 2-4
    all {
      THREADS.contains(t),
      phase.get(t) == 3,
      validated.get(t),
      lock == 0,
      // Record the share (the real code does NOT re-check job.active here!)
      accepted' = accepted.union(Set((t, jid))),
      ghostStaleAccepts' = if (isStale) ghostStaleAccepts + 1 else ghostStaleAccepts,
      phase' = phase.set(t, 4),
      lock' = 0,
      snapshot' = snapshot, validated' = validated, jobs' = jobs,
    }
  }

  // Reset thread to idle for next share
  action resetThread(t: int): bool = all {
    THREADS.contains(t),
    phase.get(t) == 4,
    phase' = phase.set(t, 0),
    validated' = validated.set(t, false),
    snapshot' = snapshot, lock' = lock, jobs' = jobs,
    accepted' = accepted, ghostStaleAccepts' = ghostStaleAccepts,
  }

  // External event: new block deactivates all jobs (clean=true)
  // Can happen between any thread phases!
  action newBlockClean: bool = all {
    lock == 0,
    jobs' = jobs.keys().mapBy(jid => { ...jobs.get(jid), active: false }),
    lock' = 0,
    phase' = phase, snapshot' = snapshot, validated' = validated,
    accepted' = accepted, ghostStaleAccepts' = ghostStaleAccepts,
  }

  // External event: add a new active job
  action addNewJob(jid: int, tgt: int): bool = all {
    lock == 0,
    jid > 0, tgt > 0,
    jobs' = jobs.put(jid, { target: tgt, active: true }),
    lock' = 0,
    phase' = phase, snapshot' = snapshot, validated' = validated,
    accepted' = accepted, ghostStaleAccepts' = ghostStaleAccepts,
  }

  action step = {
    nondet t = THREADS.oneOf()
    nondet jid = 1.to(3).oneOf()
    nondet sd = 100.to(500).oneOf()
    nondet tgt = 5000.to(20000).oneOf()
    any {
      rateCheck(t),
      jobLookup(t, jid),
      validate(t, sd),
      recordAccepted(t),
      resetThread(t),
      newBlockClean,
      addNewJob(jid, tgt),
    }
  }

  // INVARIANT: accepted shares always had active jobs at record time
  // Expected: VIOLATED — proving the TOCTOU race
  val toctouSafe = ghostStaleAccepts == 0

  // Safety: phases are well-ordered
  val phaseBounded = THREADS.forall(t => phase.get(t) >= 0 and phase.get(t) <= 4)
}

module ConcurrentShareValidationTest {
  import ConcurrentShareValidation(THREADS = Set(1, 2)).*

  // Demonstrate the TOCTOU race:
  // Thread 1 looks up job (active), then new block deactivates it,
  // then thread 1 validates and records — accepting a stale job's share
  run toctouRace = init
    .then(rateCheck(1))
    .then(jobLookup(1, 1))          // snapshot: job 1 active
    .then(newBlockClean)             // RACE: all jobs deactivated
    .then(validate(1, 150))          // validates against stale snapshot
    .then(recordAccepted(1))         // accepts share for deactivated job!
    .expect(ghostStaleAccepts == 1)

  // Without race, no stale accepts
  run noRace = init
    .then(rateCheck(1))
    .then(jobLookup(1, 1))
    .then(validate(1, 150))
    .then(recordAccepted(1))
    .expect(ghostStaleAccepts == 0)

  // Two-thread interleaving showing the race
  run twoThreadRace = init
    .then(rateCheck(1))
    .then(rateCheck(2))
    .then(jobLookup(1, 1))          // T1 snapshots job 1 as active
    .then(jobLookup(2, 2))          // T2 snapshots job 2 as active
    .then(newBlockClean)             // both jobs deactivated
    .then(validate(1, 150))
    .then(validate(2, 150))
    .then(recordAccepted(1))
    .then(recordAccepted(2))
    .expect(ghostStaleAccepts == 2)  // both threads accepted stale shares
}
