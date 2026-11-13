{-# LANGUAGE AllowAmbiguousTypes #-}
{-# LANGUAGE BangPatterns #-}
{-# LANGUAGE DataKinds #-}
{-# LANGUAGE FlexibleInstances #-}
{-# LANGUAGE LambdaCase #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE ScopedTypeVariables #-}
{-# LANGUAGE TypeApplications #-}
{-# LANGUAGE TypeFamilies #-}

module Cardano.Ledger.Binary.Decoding.Decoder (
  -- * Decoders
  Decoder,
  toPlainDecoder,
  fromPlainDecoder,
  withPlainDecoder,
  enforceDecoderVersion,
  getOriginalBytes,
  originalBytesExpectedFailureMessage,
  decodeDirectFastPath,
  plainWithDirect,
  DecoderError (..),
  C.ByteOffset,
  ByteArray (..),
  C.DecodeAction (..),
  C.TokenType (..),
  decodeFixedSized,

  -- ** Versioning
  getDecoderVersion,
  ifDecoderVersionAtLeast,
  whenDecoderVersionAtLeast,
  unlessDecoderVersionAtLeast,

  -- ** Error reporting
  cborError,
  toCborError,
  showDecoderError,
  invalidKey,
  assertTag,
  enforceSize,
  matchSize,

  -- ** Compatibility tools
  allowTag,

  -- ** Custom decoders
  decodeVersion,
  decodeRational,
  decodeRationalWithTag,
  decodeRecordNamed,
  decodeRecordNamedT,
  decodeRecordSum,
  decodeListLike,
  decodeListLikeT,
  decodeEnumBounded,
  decodeWithOrigin,

  -- *** Containers
  decodeMaybe,
  decodeNullMaybe,
  decodeStrictMaybe,
  decodeNullStrictMaybe,
  decodeEither,
  decodeList,
  decodeNonEmptyList,
  decodeVector,
  decodeSet,
  setTag,
  decodeIntMap,
  decodeMap,
  decodeMapByKey,
  decodeMapLikeEnforceNoDuplicates,
  decodeVMap,
  decodeSeq,
  decodeStrictSeq,
  decodeSetTag,
  decodeListLikeWithCount,
  decodeListLikeWithCountT,
  decodeSetLikeEnforceNoDuplicates,
  decodeNonEmptySetLikeEnforceNoDuplicates,
  decodeListLikeEnforceNoDuplicates,
  decodeMapContents,
  decodeSparseKeyed,
  decodeAccA,
  failOnNull,
  failOnMempty,

  -- **** Applicaitve
  decodeMapTraverse,
  decodeMapTraverseWithDuplicates,
  decodeMapContentsTraverse,

  -- *** Time
  decodeUTCTime,

  -- ** Lifted @cborg@ decoders
  decodeBool,
  decodeBreakOr,
  decodeByteArray,
  decodeByteArrayCanonical,
  decodeBytes,
  decodeBytesCanonical,
  decodeBytesIndef,
  decodeDouble,
  decodeDoubleCanonical,
  decodeFloat,
  decodeFloat16Canonical,
  decodeFloatCanonical,
  decodeInt,
  decodeInt16,
  decodeInt16Canonical,
  decodeInt32,
  decodeInt32Canonical,
  decodeInt64,
  decodeInt64Canonical,
  decodeInt8,
  decodeInt8Canonical,
  decodeIntCanonical,
  decodeInteger,
  decodeIntegerCanonical,
  decodeNatural,
  decodeListLen,
  decodeListLenCanonical,
  decodeListLenCanonicalOf,
  decodeListLenIndef,
  decodeListLenOf,
  decodeListLenOrIndef,
  decodeMapLen,
  decodeMapLenCanonical,
  decodeMapLenIndef,
  decodeMapLenOrIndef,
  decodeNegWord,
  decodeNegWord64,
  decodeNegWord64Canonical,
  decodeNegWordCanonical,
  decodeNull,
  decodeSequenceLenIndef,
  decodeSequenceLenN,
  decodeSimple,
  decodeSimpleCanonical,
  decodeString,
  decodeStringCanonical,
  decodeStringIndef,
  decodeStringIndefLen,
  decodeStringDefOrIndef,
  decodeBytesIndefLen,
  decodeBytesDefOrIndef,
  decodeByteArrayIndefLen,
  decodeByteArrayDefOrIndef,
  decodeTag,
  decodeTag64,
  decodeTag64Canonical,
  decodeTagCanonical,
  decodeUtf8ByteArray,
  decodeUtf8ByteArrayCanonical,
  decodeWithByteSpan,
  decodeWord,
  decodeWord16,
  decodeWord16Canonical,
  decodeWord32,
  decodeWord32Canonical,
  decodeWord64,
  decodeWord64Canonical,
  decodeWord8,
  decodeWord8Canonical,
  decodeWordCanonical,
  decodeWordCanonicalOf,
  decodeWordOf,
  decodeTerm,
  peekAvailable,
  peekByteOffset,
  peekTokenType,
  liftST,
) where

import Cardano.Base.Typeable (TypeName)
import qualified Cardano.Binary.FixedSizeCodec as FSC
import Cardano.Ledger.Binary.Plain (
  DecoderError (..),
  cborError,
  invalidKey,
  showDecoderError,
  toCborError,
 )
import qualified Cardano.Ledger.Binary.Plain as Plain (decodeBytes)
import Cardano.Ledger.Binary.Version (Version, mkVersion32, natVersion)
import Cardano.Slotting.Slot (WithOrigin, withOriginFromMaybe)
import Codec.CBOR.ByteArray (ByteArray (..))
import qualified Codec.CBOR.Decoding as C (
  ByteOffset,
  DecodeAction (..),
  Decoder,
  TokenType (..),
  decodeBool,
  decodeBreakOr,
  decodeByteArray,
  decodeByteArrayCanonical,
  decodeBytesCanonical,
  decodeBytesIndef,
  decodeDouble,
  decodeDoubleCanonical,
  decodeFloat,
  decodeFloat16Canonical,
  decodeFloatCanonical,
  decodeInt,
  decodeInt16,
  decodeInt16Canonical,
  decodeInt32,
  decodeInt32Canonical,
  decodeInt64,
  decodeInt64Canonical,
  decodeInt8,
  decodeInt8Canonical,
  decodeIntCanonical,
  decodeInteger,
  decodeIntegerCanonical,
  decodeListLen,
  decodeListLenCanonical,
  decodeListLenCanonicalOf,
  decodeListLenIndef,
  decodeListLenOf,
  decodeListLenOrIndef,
  decodeMapLen,
  decodeMapLenCanonical,
  decodeMapLenIndef,
  decodeMapLenOrIndef,
  decodeNegWord,
  decodeNegWord64,
  decodeNegWord64Canonical,
  decodeNegWordCanonical,
  decodeNull,
  decodeSequenceLenIndef,
  decodeSequenceLenN,
  decodeSimple,
  decodeSimpleCanonical,
  decodeString,
  decodeStringCanonical,
  decodeStringIndef,
  decodeTag,
  decodeTag64,
  decodeTag64Canonical,
  decodeTagCanonical,
  decodeUtf8ByteArray,
  decodeUtf8ByteArrayCanonical,
  decodeWithByteSpan,
  decodeWord,
  decodeWord16,
  decodeWord16Canonical,
  decodeWord32,
  decodeWord32Canonical,
  decodeWord64,
  decodeWord64Canonical,
  decodeWord8,
  decodeWord8Canonical,
  decodeWordCanonical,
  decodeWordCanonicalOf,
  decodeWordOf,
  liftST,
  peekAvailable,
  peekByteOffset,
  peekTokenType,
 )
import Codec.CBOR.Read as Read (DeserialiseFailure, IDecode (..), deserialiseIncremental)
import qualified Codec.CBOR.Term as C (Term (..), decodeTerm)
import Control.Monad
import Control.Monad.ST (ST)
import Control.Monad.Trans (MonadTrans (..))
import Control.Monad.Trans.Identity (IdentityT (runIdentityT))
import Data.Bits (finiteBitSize, shiftL, shiftR, (.&.), (.|.))
import Data.ByteString (ByteString)
import qualified Data.ByteString as BS
import qualified Data.ByteString.Builder as BSB
import qualified Data.ByteString.Lazy as BSL
import qualified Data.ByteString.Lazy as LBS
import Data.ByteString.Short (ShortByteString (SBS), toShort)
import Data.Functor.Compose (Compose (..))
import Data.Int (Int16, Int32, Int64, Int8)
import qualified Data.IntMap as IntMap
import Data.List (intercalate)
import qualified Data.List.NonEmpty as NE (NonEmpty, nonEmpty)
import qualified Data.Map.Strict as Map
import qualified Data.Primitive.ByteArray as Prim
import Data.Maybe.Strict (StrictMaybe (..), maybeToStrictMaybe)
import Data.Ratio ((%))
import qualified Data.Sequence as Seq
import qualified Data.Sequence.Strict as SSeq
import qualified Data.Set as Set
import qualified Data.Text as Text
import qualified Data.Text.Encoding as Text (decodeUtf8')
import qualified Data.Text.Lazy as LT
import qualified Data.Text.Lazy.Builder as B
import Data.Time.Calendar.OrdinalDate (fromOrdinalDate)
import Data.Time.Clock (UTCTime (..), picosecondsToDiffTime)
import Data.Typeable (Proxy (Proxy), Typeable, typeRep)
import qualified Data.VMap as VMap
import qualified Data.Vector.Generic as VG
import Data.Word (Word16, Word32, Word64, Word8)
import Numeric.Natural (Natural)
import Prelude hiding (decodeFloat)

--------------------------------------------------------------------------------
-- Versioned Decoder
--------------------------------------------------------------------------------

-- | A versioned decoder, held in two forms that decode the same thing.
--
-- * `runDecoder` is the decoder proper: a cborg `C.Decoder` that is given the
--   original bytes and the version. It is the specification, and the only form
--   that ever reports a failure.
--
-- * `runDirect` reads the same value straight out of one strict input chunk at
--   a byte offset. It may only answer `DOk` where `runDecoder`, run over the
--   same bytes from the same position, succeeds with an equal value after
--   consuming the same number of bytes. Whenever it cannot promise that (an
--   encoding it does not handle, a truncated token, any failure) it answers
--   `DBail`, and `decodeDirectFastPath`'s caller throws the attempt away and runs
--   `runDecoder` over the whole input instead. So accept/reject and every error
--   are always decided by the cborg path.
data Decoder s a = Decoder
  { runDecoder :: Maybe BSL.ByteString -> Version -> C.Decoder s a
  , runDirect :: Version -> ByteString -> Int -> ST s (DRes a)
  }

-- | Outcome of a direct decoder: the offset after the value and the value, or
-- no answer.
data DRes a
  = DOk !Int a
  | DBail

-- The mapped function is applied as soon as the argument has been decoded,
-- instead of leaving one updatable thunk per `fmap`/`<*>` for `rnf` (or a
-- strict constructor field) to enter later. `<$` keeps its default, lazy
-- meaning.
instance Functor (Decoder s) where
  fmap f d =
    Decoder
      (\bsl v -> runDecoder d bsl v >>= \x -> pure $! f x)
      ( \v bs off ->
          runDirect d v bs off >>= \case
            DOk off' x -> let !y = f x in pure (DOk off' y)
            DBail -> pure DBail
      )
  {-# INLINE fmap #-}
  x <$ d =
    Decoder
      (\bsl v -> x <$ runDecoder d bsl v)
      ( \v bs off ->
          runDirect d v bs off >>= \case
            DOk off' _ -> pure (DOk off' x)
            DBail -> pure DBail
      )
  {-# INLINE (<$) #-}

instance Applicative (Decoder s) where
  pure x = Decoder (\_ _ -> pure x) (\_ _ off -> pure (DOk off x))
  {-# INLINE pure #-}
  df <*> dx =
    Decoder
      ( \bsl v -> do
          h <- runDecoder df bsl v
          x <- runDecoder dx bsl v
          pure $! h x
      )
      ( \v bs off ->
          runDirect df v bs off >>= \case
            DBail -> pure DBail
            DOk off1 h ->
              runDirect dx v bs off1 >>= \case
                DBail -> pure DBail
                DOk off2 x -> let !y = h x in pure (DOk off2 y)
      )
  {-# INLINE (<*>) #-}
  da *> db =
    Decoder
      (\bsl v -> runDecoder da bsl v *> runDecoder db bsl v)
      ( \v bs off ->
          runDirect da v bs off >>= \case
            DBail -> pure DBail
            DOk off1 _ -> runDirect db v bs off1
      )
  {-# INLINE (*>) #-}

instance Monad (Decoder s) where
  d >>= g =
    Decoder
      ( \bsl v -> do
          x <- runDecoder d bsl v
          runDecoder (g x) bsl v
      )
      ( \v bs off ->
          runDirect d v bs off >>= \case
            DBail -> pure DBail
            DOk off1 x -> runDirect (g x) v bs off1
      )
  {-# INLINE (>>=) #-}

instance MonadFail (Decoder s) where
  fail msg = Decoder (\_ _ -> fail msg) (\_ _ _ -> pure DBail)
  {-# INLINE fail #-}

-- | Promote a regular `C.Decoder` to a versioned one. Which means it will work for all
-- versions. The decoder must not read the byte offset or the available input
-- (`C.peekByteOffset`, `C.decodeWithByteSpan`, `C.peekAvailable`): its direct form runs
-- it over the rest of the input chunk, where those are relative to that rest.
fromPlainDecoder :: C.Decoder s a -> Decoder s a
fromPlainDecoder d = Decoder (\_ _ -> d) (\_ bs off -> bridgePlainDecoder d bs off)
{-# INLINE fromPlainDecoder #-}

-- | The direct form of an arbitrary cborg decoder: run it with cborg itself over
-- the rest of the chunk. A cborg decoder does not depend on what was decoded
-- before it, so when this succeeds it has consumed the same bytes and produced the
-- same value as it would have in the middle of the whole input. Anything else
-- (a failure, or a request for more input than the chunk holds) is `DBail`.
bridgePlainDecoder :: C.Decoder s a -> ByteString -> Int -> ST s (DRes a)
bridgePlainDecoder d bs off =
  deserialiseIncremental d >>= \case
    Done _ _ x -> pure (DOk off x)
    Fail _ _ failure -> pure (bridgeFailed failure)
    Partial k
      | off >= BS.length bs -> pure DBail
      | otherwise ->
          k (Just (BS.drop off bs)) >>= \case
            Done _ consumed x -> pure (DOk (off + fromIntegral consumed) x)
            Fail _ _ failure -> pure (bridgeFailed failure)
            Partial _ -> pure DBail
{-# NOINLINE bridgePlainDecoder #-}

bridgeFailed :: DeserialiseFailure -> DRes a
bridgeFailed _ = DBail

-- | A cborg primitive together with a direct reading of the same token. Where the
-- direct reading declines, the cborg primitive itself is run at that position.
-- cborg hands every token's value to its continuation already evaluated, so the
-- direct reading's value is forced here too: a decoded value must never reach a
-- lazy field as a thunk where the cborg form would have put a value.
directPrim :: C.Decoder s a -> (ByteString -> Int -> DRes a) -> Decoder s a
directPrim d p =
  Decoder
    (\_ _ -> d)
    ( \_ bs off -> case p bs off of
        DBail -> bridgePlainDecoder d bs off
        DOk off' x -> x `seq` pure (DOk off' x)
    )
{-# INLINE directPrim #-}

-- | `directPrim` for a peek, whose result is only ever scrutinised: it is left
-- unforced so that the reading and the scrutiny can fuse.
directPeek :: C.Decoder s a -> (ByteString -> Int -> DRes a) -> Decoder s a
directPeek d p =
  Decoder
    (\_ _ -> d)
    ( \_ bs off -> case p bs off of
        DBail -> bridgePlainDecoder d bs off
        r -> pure r
    )
{-# INLINE directPeek #-}

-- | A cborg decoder paired with the direct form of a `Decoder` that reads the same
-- value. The cborg decoder stays the specification; the second argument must
-- succeed only where it does, with the same value and length.
plainWithDirect :: C.Decoder s a -> Decoder s a -> Decoder s a
plainWithDirect plain d = Decoder (\_ _ -> plain) (runDirect d)
{-# INLINE plainWithDirect #-}

-- | Try to decode a whole strict input with the direct form of the decoder.
-- `Nothing` means no answer: decode with `toPlainDecoder` instead. `Just` is
-- the value and the bytes left over after it.
decodeDirectFastPath :: Version -> ByteString -> Decoder s a -> ST s (Maybe (a, ByteString))
decodeDirectFastPath v bs d =
  runDirect d v bs 0 >>= \case
    DOk off x -> pure (Just (x, BS.drop off bs))
    DBail -> pure Nothing
{-# INLINE decodeDirectFastPath #-}

-- | Extract the underlying `C.Decoder` by optionally supplying the original bytes and
-- specifying the concrete version to be used.
toPlainDecoder ::
  -- | Some decoders require the original bytes to be supplied as well. Such decoders will
  -- fail whenever `Nothing` is supplied.
  Maybe BSL.ByteString ->
  Version ->
  Decoder s a ->
  C.Decoder s a
toPlainDecoder bsl v d = runDecoder d bsl v
{-# INLINE toPlainDecoder #-}

-- | Use the supplied decoder as a plain decoder with current version.
withPlainDecoder :: Decoder s a -> (C.Decoder s a -> C.Decoder s b) -> Decoder s b
withPlainDecoder vd f = Decoder (\bsl -> f . runDecoder vd bsl) (\_ _ _ -> pure DBail)
{-# INLINE withPlainDecoder #-}

-- | Ignore the current version of the decoder and enforce the supplied one instead.
enforceDecoderVersion :: Version -> Decoder s a -> Decoder s a
enforceDecoderVersion version d =
  Decoder
    (\bsl _ -> runDecoder d bsl version)
    (\_ bs off -> runDirect d version bs off)
{-# INLINE enforceDecoderVersion #-}

decodeBytesDefinite :: Decoder s ByteString
decodeBytesDefinite = directPrim Plain.decodeBytes dBytes

-- | Decode a bytestring, accepting both definite and indefinite length encodings. Chunks
-- of an indefinite length bytestring are concatenated together.
decodeBytes :: Decoder s ByteString
decodeBytes =
  ifDecoderVersionAtLeast
    (natVersion @12)
    decodeBytesDefOrIndef
    decodeBytesDefinite

decodeFixedSized :: FSC.FixedSizeCodec a => Decoder s a
decodeFixedSized = FSC.rawDecodeFixedSized =<< decodeBytes

-- | Lookup the original bytes that are being used for deserialization. This action will
-- fail deserialization whenever original bytes are not available.
getOriginalBytes :: Decoder s BSL.ByteString
getOriginalBytes =
  Decoder
    ( \maybeBytes _ ->
        case maybeBytes of
          Nothing -> fail originalBytesExpectedFailureMessage
          Just bsl -> pure bsl
    )
    -- The direct form only ever runs over the whole original input, held in one chunk.
    (\_ bs off -> let !original = BSL.fromStrict bs in pure (DOk off original))

-- | This is the message that will be reported by `getOriginalBytes` when original bytes are not
-- provided. It is defined as a separate biding for testing.
originalBytesExpectedFailureMessage :: String
originalBytesExpectedFailureMessage =
  "Decoder was expected to provide the original ByteString"

--------------------------------------------------------------------------------
-- Working with current decoder version
--------------------------------------------------------------------------------

-- | Extract current version of the decoder
--
-- >>> :set -XOverloadedStrings -XTypeApplications -XDataKinds
-- >>> import Cardano.Ledger.Binary.Decoding
-- >>> decodeFullDecoder (natVersion @3) "Version" getDecoderVersion ""
-- Right (Version 3)
getDecoderVersion :: Decoder s Version
getDecoderVersion = Decoder (\_ -> pure) (\v _ off -> pure (DOk off v))
{-# INLINE getDecoderVersion #-}

-- | Conditionally choose the newer or older decoder, depending on the current
-- version. Version in the context of encoders/decoders is the major protocol
-- version. Supplied version acts as a pivot.
--
-- =====__Example__
--
-- Let's say prior to the version 2 some type `Foo` was backed by `Word16`, but at the 2nd
-- version onwards it was switched to `Word32` instead. In order to support both versions,
-- we change the type, but we also use this condition to keep backwards compatibility of
-- the decoder:
--
-- >>> :set -XTypeApplications -XDataKinds
-- >>> newtype Foo = Foo Word32
-- >>> decFoo = Foo <$> ifDecoderVersionAtLeast (natVersion @2) decodeWord32 (fromIntegral <$> decodeWord16)
ifDecoderVersionAtLeast ::
  Version ->
  -- | Use this decoder if current decoder version is larger or equal to the supplied
  -- `Version`
  Decoder s a ->
  -- | Use this decoder if current decoder version is lower than the supplied `Version`
  Decoder s a ->
  Decoder s a
ifDecoderVersionAtLeast atLeast newerDecoder olderDecoder = do
  cur <- getDecoderVersion
  if cur >= atLeast
    then newerDecoder
    else olderDecoder
{-# INLINE ifDecoderVersionAtLeast #-}

-- | Optionally run a decoder depending on the current version and the supplied one.
whenDecoderVersionAtLeast ::
  Version ->
  -- | Run this decoder whenever current decoder version is larger or equal to the supplied
  -- `Version`
  Decoder s a ->
  Decoder s ()
whenDecoderVersionAtLeast atLeast decoder = do
  cur <- getDecoderVersion
  when (cur >= atLeast) (void decoder)
{-# INLINE whenDecoderVersionAtLeast #-}

-- | Optionally run a decoder depending on the current version and the supplied one.
unlessDecoderVersionAtLeast ::
  Version ->
  -- | Run this decoder whenever current decoder version is smaller to the supplied `Version`
  Decoder s a ->
  Decoder s ()
unlessDecoderVersionAtLeast atLeast decoder = do
  cur <- getDecoderVersion
  unless (cur >= atLeast) (void decoder)

--------------------------------------------------------------------------------
-- Error reporting
--------------------------------------------------------------------------------

decodeVersion :: Decoder s Version
decodeVersion = decodeWord32 >>= mkVersion32
{-# INLINE decodeVersion #-}

-- | `Decoder` for `Rational`. Versions variance:
--
-- * [>= 9] - Allows variable as well as exact list length encoding. Consumes tag 30 if
--   one is present, but does not enforce it.
--
-- * [>= 2] - Allows variable as well as exact list length encoding.
--
-- * [== 1] - Expects exact list length encoding.
decodeRational :: Decoder s Rational
decodeRational =
  ifDecoderVersionAtLeast
    (natVersion @9)
    (allowTag 30 >> decodeRationalWithoutTag)
    ( ifDecoderVersionAtLeast
        (natVersion @2)
        decodeRationalWithoutTag
        decodeRationalFixedSizeTuple
    )
  where
    decodeRationalFixedSizeTuple = do
      enforceSize "Rational" 2
      n <- decodeInteger
      d <- decodeInteger
      if d <= 0
        then cborError $ DecoderErrorCustom "Rational" "invalid denominator"
        else return $! n % d
    {-# INLINE decodeRationalFixedSizeTuple #-}
{-# INLINE decodeRational #-}

-- | Future `Decoder` for `Rational` type. This decoder will be applied in future and is
-- prepared here as use case on how to do upgrades to serialization. Versions variance:
--
-- * [>= 10] - Enforces tag 30
--
-- * [>= 9] - Allows variable as well as exact list length encoding. Consumes tag 30 if
--   one is present, but does not enforce it.
--
-- * [>= 2] - Allows variable as well as exact list length encoding.
--
-- * [== 1] - Expects exact list length encoding.
_decodeRationalFuture :: Decoder s Rational
_decodeRationalFuture = do
  -- We are not using `natVersion` because these versions aren't yet supported.
  v9 <- mkVersion32 9
  v10 <- mkVersion32 10
  ifDecoderVersionAtLeast
    v10
    decodeRationalWithTag
    ( ifDecoderVersionAtLeast
        v9
        (allowTag 30 >> decodeRational)
        decodeRational
    )

-- | Enforces tag 30 to indicate a rational number, as per tag assignment:
-- <https://www.iana.org/assignments/cbor-tags/cbor-tags.xhtml>
--
-- <https://peteroupc.github.io/CBOR/rational.html>
decodeRationalWithTag :: Decoder s Rational
decodeRationalWithTag = assertTag 30 >> decodeRationalWithoutTag
{-# INLINE decodeRationalWithTag #-}

decodeRationalWithoutTag :: Decoder s Rational
decodeRationalWithoutTag = do
  (numValues, values) <- decodeCollectionWithLen decodeListLenOrIndef decodeInteger
  case values of
    [n, d] -> do
      when (d == 0) (fail "Denominator cannot be zero")
      pure $! n % d
    _ -> cborError $ DecoderErrorSizeMismatch "Rational" 2 numValues
{-# INLINE decodeRationalWithoutTag #-}

--------------------------------------------------------------------------------
-- Containers
--------------------------------------------------------------------------------

-- | @'Decoder'@ for list.
--
-- * [>= 2] - Allows variable as well as exact list length encoding.
--
-- * [< 2] - Expects variable list length encoding
decodeList :: Decoder s a -> Decoder s [a]
decodeList decodeValue =
  ifDecoderVersionAtLeast
    (natVersion @2)
    (decodeCollection decodeListLenOrIndef decodeValue)
    (decodeListWith decodeValue)
{-# INLINE decodeList #-}

decodeNonEmptyList :: Decoder s a -> Decoder s (NE.NonEmpty a)
decodeNonEmptyList decodeValue = do
  xs <- decodeList decodeValue
  case NE.nonEmpty xs of
    Nothing -> fail "Empty list found, expected non-empty"
    Just ne -> pure ne
{-# INLINE decodeNonEmptyList #-}

-- | @'Decoder'@ for list.
decodeListWith :: Decoder s a -> Decoder s [a]
decodeListWith decodeValue = do
  decodeListLenIndef
  decodeSequenceLenIndef (flip (:)) [] reverse decodeValue
{-# INLINE decodeListWith #-}

-- | `Decoder` for `Maybe`. Versions variance:
--
-- * [>= 2] - Allows variable as well as exact list length encoding.
--
-- * [< 2] - Expects exact list length encoding
decodeMaybe :: Decoder s a -> Decoder s (Maybe a)
decodeMaybe decodeValue = do
  ifDecoderVersionAtLeast
    (natVersion @2)
    (decodeMaybeVarLen decodeValue)
    (decodeMaybeExactLen decodeValue)
{-# INLINE decodeMaybe #-}

decodeMaybeExactLen :: Decoder s a -> Decoder s (Maybe a)
decodeMaybeExactLen decodeValue = do
  n <- decodeListLen
  case n of
    0 -> return Nothing
    1 -> do
      !x <- decodeValue
      return (Just x)
    _ -> fail "too many elements while decoding Maybe."
{-# INLINE decodeMaybeExactLen #-}

decodeMaybeVarLen :: Decoder s a -> Decoder s (Maybe a)
decodeMaybeVarLen decodeValue = do
  maybeLength <- decodeListLenOrIndef
  case maybeLength of
    Just 0 -> pure Nothing
    Just 1 -> Just <$!> decodeValue
    Just _ -> fail "too many elements in length-style decoding of Maybe."
    Nothing -> do
      isBreak <- decodeBreakOr
      if isBreak
        then pure Nothing
        else do
          !x <- decodeValue
          isBreak2 <- decodeBreakOr
          unless isBreak2 $
            fail "too many elements in break-style decoding of Maybe."
          pure (Just x)
{-# INLINE decodeMaybeVarLen #-}

-- | Alternative way to decode a Maybe type.
--
-- /Note/ - this is not the default method for decoding `Maybe`, use `decodeMaybe` instead.
decodeNullMaybe :: Decoder s a -> Decoder s (Maybe a)
decodeNullMaybe decoder = do
  peekTokenType >>= \case
    C.TypeNull -> do
      decodeNull
      pure Nothing
    _ -> Just <$> decoder
{-# INLINE decodeNullMaybe #-}

-- | Unlike `decodeMaybe` this allows variable as well as exact list length encoding for
-- all versions, because Byron never used `StrictMaybe` type.
decodeStrictMaybe :: Decoder s a -> Decoder s (StrictMaybe a)
decodeStrictMaybe = fmap maybeToStrictMaybe . decodeMaybeVarLen
{-# INLINE decodeStrictMaybe #-}

-- | Alternative way to decode a `StrictMaybe` type.
--
-- /Note/ - this is not the default method for decoding `StrictMaybe`, use
-- `decodeStrictMaybe` instead.
decodeNullStrictMaybe :: Decoder s a -> Decoder s (StrictMaybe a)
decodeNullStrictMaybe decoder = do
  peekTokenType >>= \case
    C.TypeNull -> do
      decodeNull
      pure SNothing
    _ -> SJust <$> decoder
{-# INLINE decodeNullStrictMaybe #-}

decodeEither :: Decoder s a -> Decoder s b -> Decoder s (Either a b)
decodeEither decodeLeft decodeRight = do
  decodeListLenOf 2
  t <- decodeWord
  case t of
    0 -> Left <$> decodeLeft
    1 -> Right <$> decodeRight
    _ -> cborError $ DecoderErrorUnknownTag "Either" t
{-# INLINE decodeEither #-}

decodeRecordNamed :: Text.Text -> (a -> Int) -> Decoder s a -> Decoder s a
decodeRecordNamed name getRecordSize decoder = do
  runIdentityT $ decodeRecordNamedT name getRecordSize (lift decoder)
{-# INLINE decodeRecordNamed #-}

decodeRecordNamedT ::
  (MonadTrans m, Monad (m (Decoder s))) =>
  Text.Text ->
  (a -> Int) ->
  m (Decoder s) a ->
  m (Decoder s) a
decodeRecordNamedT name getRecordSize decoder =
  decodeListLikeT name decoder $ \result n ->
    lift $ matchSize ("Record " <> name) n (getRecordSize result)
{-# INLINE decodeRecordNamedT #-}

decodeRecordSum :: Text.Text -> (Word -> Decoder s (Int, a)) -> Decoder s a
decodeRecordSum name decoder =
  snd <$> do
    decodeListLike name (decodeWord >>= decoder) $ \(size, _) n ->
      matchSize (Text.pack "Sum " <> name) size n
{-# INLINE decodeRecordSum #-}

-- | Use this decoder for any list like structure that accepts fixed or variable list
-- length encoding.
decodeListLike ::
  -- | Name for error reporting
  Text.Text ->
  -- | Decoder for the datastructure itself
  Decoder s a ->
  -- | In case when length was encoded, act upon it.
  (a -> Int -> Decoder s ()) ->
  Decoder s a
decodeListLike name decoder actOnLength =
  runIdentityT $ decodeListLikeT name (lift decoder) (\r i -> lift (actOnLength r i))
{-# INLINE decodeListLike #-}

decodeListLikeT ::
  (MonadTrans m, Monad (m (Decoder s))) =>
  -- | Name for error reporting
  Text.Text ->
  -- | Decoder for the datastructure itself
  m (Decoder s) a ->
  -- | In case when length was encoded, act upon it.
  (a -> Int -> m (Decoder s) ()) ->
  m (Decoder s) a
decodeListLikeT name decoder actOnLength = do
  lenOrIndef <- lift decodeListLenOrIndef
  result <- decoder
  case lenOrIndef of
    Just n -> actOnLength result n
    Nothing -> lift $ do
      isBreak <- decodeBreakOr
      unless isBreak $ cborError $ DecoderErrorCustom name "Excess terms in array"
  pure result
{-# INLINE decodeListLikeT #-}

decodeEnumBounded :: forall a s. (Enum a, Bounded a, Typeable a) => Decoder s a
decodeEnumBounded = do
  n <- decodeInt
  if fromEnum (minBound :: a) <= n && n <= fromEnum (maxBound :: a)
    then pure $ toEnum n
    else
      fail $
        "Failed to decode an Enum: " <> show n <> " for TypeRep: " <> show (typeRep (Proxy @a))
{-# INLINE decodeEnumBounded #-}

decodeWithOrigin :: Decoder s a -> Decoder s (WithOrigin a)
decodeWithOrigin f = withOriginFromMaybe <$> decodeMaybe f
{-# INLINE decodeWithOrigin #-}

--------------------------------------------------------------------------------
-- Decoder for Map
--------------------------------------------------------------------------------

-- | Checks canonicity by comparing the new key being decoded with
--   the previous one, to enfore these are sorted the correct way.
--   See: https://tools.ietf.org/html/rfc7049#section-3.9
--   "[..]The keys in every map must be sorted lowest value to highest.[...]"
--
-- In other words this decoder enforces strict monotonically increasing order on keys. It
-- also uses exact map length encoding.
decodeMapSkel ::
  forall k m v s.
  Ord k =>
  -- | Decoded list is guaranteed to be sorted on keys in descending order without any
  -- duplicate keys.
  ([(k, v)] -> m) ->
  -- | Decoder for keys and values
  Decoder s (k, v) ->
  Decoder s m
decodeMapSkel fromDistinctDescList decodeKeyValue = do
  n <- decodeMapLen
  fromDistinctDescList <$> case n of
    0 -> return []
    _ -> do
      (firstKey, firstValue) <- decodeKeyValue
      decodeEntries (n - 1) firstKey [(firstKey, firstValue)]
  where
    -- Decode all the entries, enforcing canonicity by ensuring that the
    -- previous key is smaller than the next one.
    decodeEntries :: Int -> k -> [(k, v)] -> Decoder s [(k, v)]
    decodeEntries 0 _ acc = pure acc
    decodeEntries !remainingPairs previousKey !acc = do
      p@(newKey, _) <- decodeKeyValue
      -- Order of keys needs to be strictly increasing, because otherwise it's
      -- possible to supply lists with various amount of duplicate keys which
      -- will result in the same map as long as the last value of the given
      -- key on the list is the same in all of them.
      if newKey > previousKey
        then decodeEntries (remainingPairs - 1) newKey (p : acc)
        else cborError $ DecoderErrorCanonicityViolation "Map"
{-# INLINE decodeMapSkel #-}

decodeCollection :: Decoder s (Maybe Int) -> Decoder s a -> Decoder s [a]
decodeCollection lenOrIndef el = snd <$> decodeCollectionWithLen lenOrIndef el
{-# INLINE decodeCollection #-}

decodeCollectionWithLen ::
  Decoder s (Maybe Int) ->
  Decoder s v ->
  Decoder s (Int, [v])
decodeCollectionWithLen lenOrIndef decodeElement =
  fmap reverse <$> decodeListLikeWithCount lenOrIndef (:) (const decodeElement)
{-# INLINE decodeCollectionWithLen #-}

-- | `Decoder` for `Map.Map`. Versions variance:
--
-- * [>= 9] - Allows variable as well as exact list length encoding. Duplicate keys will
--   result in a deserialization failure
--
-- * [>= 2] - Allows variable as well as exact list length encoding. Duplicate keys are
--   silently ignored
--
-- * [< 2] - Expects exact list length encoding and enforces strict order
--   without any duplicates.
--
-- An example of how to use versioning
--
-- >>> :set -XOverloadedStrings -XTypeApplications -XDataKinds
-- >>> import Codec.CBOR.FlatTerm
-- >>> fromFlatTerm (toPlainDecoder Nothing (natVersion @1) (decodeMap decodeInt decodeBytes)) [TkMapLen 2,TkInt 1,TkBytes "Foo",TkInt 2,TkBytes "Bar"]
-- Right (fromList [(1,"Foo"),(2,"Bar")])
-- >>> fromFlatTerm (toPlainDecoder Nothing (natVersion @1) (decodeMap decodeInt decodeBytes)) [TkMapBegin,TkInt 1,TkBytes "Foo",TkInt 2,TkBytes "Bar"]
-- Left "decodeMapLen: unexpected token TkMapBegin"
-- >>> fromFlatTerm (toPlainDecoder Nothing (natVersion @2) (decodeMap decodeInt decodeBytes)) [TkMapBegin,TkInt 1,TkBytes "Foo",TkInt 2,TkBytes "Bar",TkBreak]
-- Right (fromList [(1,"Foo"),(2,"Bar")])
decodeMap ::
  Ord k =>
  Decoder s k ->
  Decoder s v ->
  Decoder s (Map.Map k v)
decodeMap decodeKey decodeValue = decodeMapByKey decodeKey (const decodeValue)
{-# INLINE decodeMap #-}

-- | Just like `decodeMap`, but also gives access to the key for the value decoder.
decodeMapByKey ::
  Ord k =>
  Decoder s k ->
  (k -> Decoder s v) ->
  Decoder s (Map.Map k v)
decodeMapByKey decodeKey decodeValue =
  ifDecoderVersionAtLeast
    (natVersion @2)
    ( ifDecoderVersionAtLeast
        (natVersion @9)
        (decodeMapLikeEnforceNoDuplicates decodeMapLenOrIndef decodeKeyValue)
        (Map.fromList <$> decodeCollection decodeMapLenOrIndef decodeKeyValue)
    )
    (decodeMapSkel Map.fromDistinctDescList decodeKeyValue)
  where
    decodeKeyValue = do
      !key <- decodeKey
      !value <- decodeValue key
      pure (key, value)
    {-# INLINE decodeKeyValue #-}
{-# INLINE decodeMapByKey #-}

-- | Similar to `decodeMapByKey`, except it gives access to the key value
-- decoder as a pair and allows for different type of length encoding
decodeMapLikeEnforceNoDuplicates ::
  Ord k =>
  Decoder s (Maybe Int) ->
  Decoder s (k, v) ->
  Decoder s (Map.Map k v)
decodeMapLikeEnforceNoDuplicates =
  decodeMapLikeEnforceNoDuplicatesInternal Map.fromList Map.size
{-# INLINE decodeMapLikeEnforceNoDuplicates #-}

decodeMapLikeEnforceNoDuplicatesInternal ::
  ([(k, v)] -> m) ->
  (m -> Int) ->
  Decoder s (Maybe Int) ->
  Decoder s (k, v) ->
  Decoder s m
decodeMapLikeEnforceNoDuplicatesInternal fromPairs size decodeLenOrIndef =
  -- We first decode into a list because most of the time the encoded Map will be in sorted
  -- order and there is a nice optimization on the `Map.fromList` that can take advantage of
  -- that fact. In case when encoded data is not sorted the penalty of going through a list
  -- is insignificant.
  decodeListLikeEnforceNoDuplicates decodeLenOrIndef (:) $ \xs ->
    let result = fromPairs (reverse xs)
     in (size result, result)
{-# INLINE decodeMapLikeEnforceNoDuplicatesInternal #-}

decodeIntMap :: Decoder s v -> Decoder s (IntMap.IntMap v)
decodeIntMap decodeValue =
  ifDecoderVersionAtLeast
    (natVersion @2)
    ( ifDecoderVersionAtLeast
        (natVersion @9)
        ( decodeMapLikeEnforceNoDuplicatesInternal
            IntMap.fromList
            IntMap.size
            decodeMapLenOrIndef
            decodeKeyValue
        )
        (IntMap.fromList <$> decodeCollection decodeMapLenOrIndef decodeKeyValue)
    )
    ( decodeMapSkel
        (IntMap.fromDistinctAscList . reverse)
        decodeKeyValue
    )
  where
    decodeKeyValue = (,) <$> decodeInt <*> decodeValue
{-# INLINE decodeIntMap #-}

-- | Decode `VMap`. Unlike `decodeMap` it does not behavee differently for
-- version prior to 2.
decodeVMap ::
  (VMap.Vector kv k, VMap.Vector vv v, Ord k) =>
  Decoder s k ->
  Decoder s v ->
  Decoder s (VMap.VMap kv vv k v)
decodeVMap decodeKey decodeValue =
  VMap.fromMap
    <$> decodeMapLikeEnforceNoDuplicates
      decodeMapLenOrIndef
      ((,) <$> decodeKey <*> decodeValue)
{-# INLINE decodeVMap #-}

-- | We stitch a `258` in from of a (Hash)Set, so that tools which
-- programmatically check for canonicity can recognise it from a normal
-- array. Why 258? This will be formalised pretty soon, but IANA allocated
-- 256...18446744073709551615 to "First come, first served":
-- https://www.iana.org/assignments/cbor-tags/cbor-tags.xhtml Currently `258` is
-- the first unassigned tag and as it requires 2 bytes to be encoded, it sounds
-- like the best fit.
--
-- <https://github.com/input-output-hk/cbor-sets-spec/blob/master/CBOR_SETS.md>
setTag :: Word
setTag = 258

decodeSetTag :: Decoder s ()
decodeSetTag = do
  t <- decodeTag
  when (t /= setTag) $ cborError $ DecoderErrorUnknownTag "Set" t
{-# INLINE decodeSetTag #-}

decodeSetSkel ::
  forall a s c.
  Ord a =>
  -- | Decoded list is guaranteed to be sorted on keys in descending order without any
  -- duplicate keys.
  ([a] -> c) ->
  Decoder s a ->
  Decoder s c
decodeSetSkel fromDistinctDescList decodeValue = do
  decodeSetTag
  n <- decodeListLen
  fromDistinctDescList <$> case n of
    0 -> return []
    _ -> do
      firstValue <- decodeValue
      decodeEntries (n - 1) firstValue [firstValue]
  where
    decodeEntries :: Int -> a -> [a] -> Decoder s [a]
    decodeEntries 0 _ acc = pure acc
    decodeEntries !remainingEntries previousValue !acc = do
      newValue <- decodeValue
      -- Order of values needs to be strictly increasing, because otherwise
      -- it's possible to supply lists with various amount of duplicates which
      -- will result in the same set.
      if newValue > previousValue
        then decodeEntries (remainingEntries - 1) newValue (newValue : acc)
        else cborError $ DecoderErrorCanonicityViolation "Set"
{-# INLINE decodeSetSkel #-}

-- | `Decoder` for `Set.Set`. Versions variance:
--
-- * [>= 9] - Allows variable as well as exact list length encoding. Duplicates are
--   not allowed. Set tag 258 is permitted, but not enforced.
--
-- * [>= 2, < 9] - Allows variable as well as exact list length encoding. Duplicates are
--   silently ignored, set tag 258 is not permitted.
--
-- * [< 2] - Expects exact list length encoding and enforces strict order
--   without any duplicates. Also enforces special set tag 258, which was
--   abandoned starting with version 2
decodeSet :: Ord a => Decoder s a -> Decoder s (Set.Set a)
decodeSet valueDecoder =
  ifDecoderVersionAtLeast
    (natVersion @2)
    ( ifDecoderVersionAtLeast
        (natVersion @9)
        (decodeSetEnforceNoDuplicates valueDecoder)
        (Set.fromList <$> decodeCollection decodeListLenOrIndef valueDecoder)
    )
    (decodeSetSkel Set.fromDistinctDescList valueDecoder)
{-# INLINE decodeSet #-}

decodeSetEnforceNoDuplicates ::
  forall s a.
  Ord a =>
  Decoder s a ->
  Decoder s (Set.Set a)
decodeSetEnforceNoDuplicates = decodeSetLikeEnforceNoDuplicates (:) $ \xs ->
  -- We first decode into a list because most of the time the encoded Set will be in sorted
  -- order and there is a nice optimization on the `Set.fromList` that can take advantage of
  -- that fact. In case when encoded data is not sorted the penalty of going through a list
  -- is insignificant.
  let result = Set.fromList (reverse xs)
   in (Set.size result, result)
{-# INLINE decodeSetEnforceNoDuplicates #-}

-- | Decode a collection of values with ability to supply length decoder. Number of
-- decoded elements will be returned together with the data structure
decodeListLikeWithCount ::
  forall s a b.
  Monoid b =>
  -- | Length decoder that produces the expected number of elements. When `Nothing` is
  -- decoded the `decodeBreakOr` will be used as termination indicator.
  Decoder s (Maybe Int) ->
  -- | Add an element into the decoded List like data structure
  (a -> b -> b) ->
  -- | Decoder for the values. Current accumulator is supplied as an argument
  (b -> Decoder s a) ->
  Decoder s (Int, b)
-- TODO: define as
-- decodeListLikeWithCount decodeLenOrIndef insert decodeElement =
--   runIndentityT $ decodeListLikeWithCountT (lift decodeLenOrIndef) insert (lift decodeElement)
-- and add a SPECIALIZE pragma
decodeListLikeWithCount decodeLenOrIndef insert decodeElement =
  Decoder (runDecoder generic) direct
  where
    generic :: Decoder s (Int, b)
    generic =
      decodeLenOrIndef >>= \case
        Just len -> loop (\x -> pure (x >= len)) 0 mempty
        Nothing -> loop (\_ -> decodeBreakOr) 0 mempty
    loop :: (Int -> Decoder s Bool) -> Int -> b -> Decoder s (Int, b)
    loop condition = go
      where
        go !count !acc = do
          shouldStop <- condition count
          if shouldStop
            then pure (count, acc)
            else do
              element <- decodeElement acc
              go (count + 1) (insert element acc)
    {-# INLINE loop #-}
    -- The direct form of `generic`, written out as a loop over the offset: the same
    -- steps in the same order (length, then per element the stop test, the element
    -- and the insert, with count and accumulator forced on entry as `go` does), but
    -- without building a `Decoder` per iteration.
    direct :: Version -> ByteString -> Int -> ST s (DRes (Int, b))
    direct v bs off0 =
      runDirect decodeLenOrIndef v bs off0 >>= \case
        DBail -> pure DBail
        DOk off1 (Just len) -> goN len 0 mempty off1
        DOk off1 Nothing -> goIndef 0 mempty off1
      where
        goN :: Int -> Int -> b -> Int -> ST s (DRes (Int, b))
        goN len !count !acc !off
          | count >= len = pure (DOk off (count, acc))
          | otherwise =
              runDirect (decodeElement acc) v bs off >>= \case
                DBail -> pure DBail
                DOk off' element -> goN len (count + 1) (insert element acc) off'
        goIndef :: Int -> b -> Int -> ST s (DRes (Int, b))
        goIndef !count !acc !off =
          runDirect decodeBreakOr v bs off >>= \case
            DBail -> pure DBail
            DOk off' True -> pure (DOk off' (count, acc))
            DOk off' False ->
              runDirect (decodeElement acc) v bs off' >>= \case
                DBail -> pure DBail
                DOk off'' element -> goIndef (count + 1) (insert element acc) off''
{-# INLINE decodeListLikeWithCount #-}

decodeListLikeWithCountT ::
  forall t s a b.
  (MonadTrans t, Monad (t (Decoder s)), Monoid b) =>
  -- | Length decoder that produces the expected number of elements. When `Nothing` is
  -- decoded the `decodeBreakOr` will be used as termination indicator.
  t (Decoder s) (Maybe Int) ->
  -- | Add an element into the decoded List like data structure
  (a -> b -> b) ->
  -- | Decoder for the values. Current accumulator is supplied as an argument
  (b -> t (Decoder s) a) ->
  t (Decoder s) (Int, b)
decodeListLikeWithCountT decodeLenOrIndef insert decodeElement = do
  decodeLenOrIndef >>= \case
    Just len -> loop (\x -> pure (x >= len)) 0 mempty
    Nothing -> loop (\_ -> lift decodeBreakOr) 0 mempty
  where
    loop condition = go
      where
        go !count !acc = do
          shouldStop <- condition count
          if shouldStop
            then pure (count, acc)
            else do
              element <- decodeElement acc
              go (count + 1) (insert element acc)
    {-# INLINE loop #-}
{-# INLINE decodeListLikeWithCountT #-}

-- | Decode a collection of values with ability to supply length decoder. Duplicates are not
-- allowed.
decodeListLikeEnforceNoDuplicates ::
  forall s a b c.
  Monoid b =>
  Decoder s (Maybe Int) ->
  -- | Add an element into the decoded List like data structure
  (a -> b -> b) ->
  -- | Get the final data structure and the number of elements it has.
  (b -> (Int, c)) ->
  Decoder s a ->
  Decoder s c
decodeListLikeEnforceNoDuplicates decodeLenOrIndef insert getFinalWithCount decodeElement = do
  (count, result) <- decodeListLikeWithCount decodeLenOrIndef insert (const decodeElement)
  let (len, finalResult) = getFinalWithCount result
  when (len /= count) $
    fail $
      "Final number of elements: "
        <> show len
        <> " does not match the total count that was decoded: "
        <> show count
  pure finalResult
{-# INLINE decodeListLikeEnforceNoDuplicates #-}

-- | Decode a Set as a either a definite or indefinite list. Duplicates are not
-- allowed. Set tag 258 is permitted, but not enforced.
decodeSetLikeEnforceNoDuplicates ::
  forall s a b c.
  Monoid b =>
  -- | Add an element into the decoded Set like data structure
  (a -> b -> b) ->
  -- | Get the final data structure from the decoded sequence of values and the number of
  -- elements it contains. This is useful when a sequence on the wire is represented by a
  -- @set@, namely no duplicates are allowed, while the Haskell representation uses some
  -- other data structure that enforces no duplicates by some other means. For example a
  -- `Map`, where keys are hashes of the values encoded on the wire. The size of the final
  -- data structure will be used to enforce the invariant that the number of elements
  -- decoded matches the final size of the Set like data structure, thus ensuring no
  -- duplicates were encountered.
  (b -> (Int, c)) ->
  Decoder s a ->
  Decoder s c
decodeSetLikeEnforceNoDuplicates insert getFinalWithLen decodeElement = do
  allowTag setTag
  decodeListLikeEnforceNoDuplicates decodeListLenOrIndef insert getFinalWithLen decodeElement
{-# INLINE decodeSetLikeEnforceNoDuplicates #-}

-- | Same as `decodeSetLikeEnforceNoDuplicates`, but also enforces that the set is non-empty.
decodeNonEmptySetLikeEnforceNoDuplicates ::
  forall s a b c.
  Monoid b =>
  -- | Add an element into the decoded Set like data structure
  (a -> b -> b) ->
  -- | Get the final data structure and the number of elements it has.
  (b -> (Int, c)) ->
  Decoder s a ->
  Decoder s c
decodeNonEmptySetLikeEnforceNoDuplicates insert getFinalWithLen decodeElement = do
  (len, result) <-
    decodeSetLikeEnforceNoDuplicates
      insert
      (\b -> let (n, c) = getFinalWithLen b in (n, (n, c)))
      decodeElement
  when (len == 0) $
    fail "Expected a non-empty set, but got an empty set"
  pure result
{-# INLINE decodeNonEmptySetLikeEnforceNoDuplicates #-}

decodeContainerSkelWithReplicate ::
  -- | How to get the size of the container
  Decoder s Int ->
  -- | replicateM for the container
  (Int -> Decoder s c) ->
  -- | concat for the container
  ([c] -> c) ->
  Decoder s c
decodeContainerSkelWithReplicate decodeLen replicateFun concatList = do
  -- Look at how much data we have at the moment and use it as the limit for
  -- the size of a single call to replicateFun. We don't want to use
  -- replicateFun directly on the result of decodeLen since this might lead to
  -- DOS attack (attacker providing a huge value for length). So if it's above
  -- our limit, we'll do manual chunking and then combine the containers into
  -- one.
  sz <- decodeLen
  limit <- peekAvailable
  if sz <= limit
    then replicateFun sz
    else do
      -- Take the max of limit and a fixed chunk size (note: limit can be
      -- 0). This basically means that the attacker can make us allocate a
      -- container of size 128 even though there's no actual input.
      let chunkSize = max limit 128
          (d, m) = sz `divMod` chunkSize
      containers <- sequence $ replicateFun m : replicate d (replicateFun chunkSize)
      return $! concatList containers
{-# INLINE decodeContainerSkelWithReplicate #-}

-- | Generic decoder for vectors. Its intended use is to allow easy
-- definition of 'Serialise' instances for custom vector
decodeVector :: VG.Vector vec a => Decoder s a -> Decoder s (vec a)
decodeVector decodeValue =
  decodeContainerSkelWithReplicate
    decodeListLen
    (`VG.replicateM` decodeValue)
    VG.concat
{-# INLINE decodeVector #-}

-- | Decoder for `Seq.Seq`. Same behavior for all versions, allows variable as
-- well as exact list length encoding
decodeSeq :: Decoder s a -> Decoder s (Seq.Seq a)
decodeSeq decoder = Seq.fromList <$> decodeCollection decodeListLenOrIndef decoder
{-# INLINE decodeSeq #-}

-- | Decoder for `SSeq.StrictSeq`. Same behavior for all versions, allows variable as
-- well as exact list length encoding.
decodeStrictSeq :: Decoder s a -> Decoder s (SSeq.StrictSeq a)
decodeStrictSeq decoder = SSeq.fromList <$> decodeCollection decodeListLenOrIndef decoder
{-# INLINE decodeStrictSeq #-}

decodeMapContents :: Decoder s a -> Decoder s [a]
decodeMapContents = decodeCollection decodeMapLenOrIndef
{-# INLINE decodeMapContents #-}

decodeMapTraverseWithDuplicates ::
  (Ord a, Applicative t) =>
  Decoder s (t a) ->
  Decoder s (t b) ->
  Decoder s (t (Map.Map a b))
decodeMapTraverseWithDuplicates decodeKey decodeValue =
  fmap Map.fromList <$> decodeMapContentsTraverse decodeKey decodeValue
{-# INLINE decodeMapTraverseWithDuplicates #-}

decodeMapTraverse ::
  (Ord a, Applicative t) =>
  Decoder s (t a) ->
  Decoder s (t b) ->
  Decoder s (t (Map.Map a b))
decodeMapTraverse = decodeMapTraverseWithDuplicates
{-# DEPRECATED decodeMapTraverse "Use decodeMapTraverseWithDuplicates or decodeMapNoDuplicatesAnn instead" #-}

decodeMapContentsTraverse ::
  Applicative t =>
  Decoder s (t a) ->
  Decoder s (t b) ->
  Decoder s (t [(a, b)])
decodeMapContentsTraverse decodeKey decodeValue =
  sequenceA <$> decodeMapContents decodeInlinedPair
  where
    decodeInlinedPair = getCompose $ (,) <$> Compose decodeKey <*> Compose decodeValue
    {-# INLINE decodeInlinedPair #-}
{-# INLINE decodeMapContentsTraverse #-}

-- | Decode a sparse-keyed CBOR map into an accumulator, dispatching per
-- key.
--
-- A /sparse-keyed map/ is the CBOR shape used throughout Cardano
-- for records where every field is optional and is identified
-- by a small non-negative integer key. CDDL examples include
-- @transaction_witness_set@, @transaction_body@, @auxiliary_data@, and
-- @protocol_param_update@. The on-the-wire shape is
--
-- @
-- { ? key0 => value0, ? key1 => value1, ... }
-- @
--
-- with no order requirement, no padding for absent fields, and a CBOR
-- map header that may be either definite-length (major type 5 with a
-- count) or indefinite-length (@0xBF@ followed by entries terminated
-- by the break byte @0xFF@). That being said, uniqueness of keys is
-- enforced upon deserialisation
--
-- The handler is invoked /after/ the helper has read the key but
-- /before/ the value bytes are consumed:
--
-- * @'Just' decoder@ supplies the decoder for the value
--   bytes. It receives the current accumulator and returns
--   the updated one, supporting both record-update style
--   (plain accumulators) and applicative threading (e.g.
--   @'Cardano.Ledger.Binary.Decoding.Annotated.Annotator' t@
--   accumulators).
-- * @'Nothing'@ signals an unknown key. The helper reports it uniformly
--   so callers do not write per-type \"unknown key\" strings.
--
-- @
-- data R = R { rA :: !(Maybe Word64), rB :: !(Maybe Word64) }
--
-- decodeR :: Decoder s R
-- decodeR = decodeSparseKeyed "R" [(0, "a")] (R Nothing Nothing) $ \r -> \case
--   0 -> Just $ do
--     !v <- decCBOR
--     pure $ r { rA = Just v }
--   1 -> Just $ do
--     !v <- decCBOR
--     pure $ r { rB = Just v }
--   _ -> Nothing
-- @
decodeSparseKeyed ::
  forall a s.
  -- | Type name used in error messages.
  TypeName a ->
  -- | Required keys with friendly names per key. After the map is fully
  -- consumed, the absence of any key in this list is reported as a
  -- failure using its name and key.
  [(Word, String)] ->
  -- | Initial accumulator value.
  a ->
  -- | Per-key handler. @'Just' d@ provides the decoder for the value
  -- that is already applied to the supplied accumulator; 'Nothing'
  -- indicates the key as unknown and is reported as decoding failure.
  (a -> Word -> Maybe (Decoder s a)) ->
  Decoder s a
decodeSparseKeyed name requiredFields initial decoderByKey = do
  (seen, acc) <-
    decodeMapLenOrIndef >>= \case
      Just len -> defLoop Set.empty initial len
      Nothing -> indefLoop Set.empty initial
  let missing =
        [ show n <> ": " <> show k
        | (k, n) <- requiredFields
        , not $ Set.member k seen
        ]
  unless (null missing) $
    failMsg $
      "Missing required field(s): " <> intercalate ", " missing
  pure acc
  where
    failMsg :: String -> Decoder s b
    failMsg msg = fail $ show name <> ": " <> msg

    defLoop :: Set.Set Word -> a -> Int -> Decoder s (Set.Set Word, a)
    defLoop !seen !acc !i
      | i <= 0 = pure (seen, acc)
      | otherwise = do
          (seen', acc') <- step seen acc
          defLoop seen' acc' (i - 1)

    indefLoop :: Set.Set Word -> a -> Decoder s (Set.Set Word, a)
    indefLoop !seen !acc =
      decodeBreakOr >>= \case
        True -> pure (seen, acc)
        False -> do
          (seen', acc') <- step seen acc
          indefLoop seen' acc'

    step :: Set.Set Word -> a -> Decoder s (Set.Set Word, a)
    step seen acc = do
      key <- decodeWord
      if Set.member key seen
        then failMsg $ "Duplicate field key " <> show key
        else case decoderByKey acc key of
          Nothing -> failMsg $ "Unknown field key " <> show key
          Just decoder -> do
            acc' <- decoder
            pure (Set.insert key seen, acc')
    {-# INLINE step #-}
{-# INLINE decodeSparseKeyed #-}

-- | Decode a value and add it to the accumulator using the
-- `Applicative` instance.
decodeAccA ::
  Applicative f =>
  -- | Applicative accumulator
  f t ->
  -- | Function to adds the decoded value to the accumulator
  (x -> t -> t) ->
  -- | Decoder of the Applicative value
  Decoder s (f x) ->
  Decoder s (f t)
decodeAccA acc setter dec = do
  !x <- dec
  pure $ setter <$> x <*> acc
{-# INLINE decodeAccA #-}

failOnNull :: (Foldable f, MonadFail m) => f a -> String -> m ()
failOnNull f msg = when (null f) $ fail msg

failOnMempty :: (Eq f, Monoid f, MonadFail m) => f -> String -> m ()
failOnMempty f msg = when (f == mempty) $ fail msg

--------------------------------------------------------------------------------
-- Time
--------------------------------------------------------------------------------

-- | `Decoder` for `UTCTime`. Versions variance:
--
-- * [>= 2] - Allows variable list length encoding, but still expects number of
--   elements to be 3.
--
-- * [< 2] - Expects exact list length encoding to be 3
decodeUTCTime :: Decoder s UTCTime
decodeUTCTime =
  ifDecoderVersionAtLeast
    (natVersion @2)
    (enforceSize "UTCTime" 3 >> timeDecoder)
    (decodeRecordNamed "UTCTime" (const 3) timeDecoder)
  where
    timeDecoder = do
      !year <- decodeInteger
      !dayOfYear <- decodeInt
      !timeOfDayPico <- decodeInteger
      return $!
        UTCTime
          (fromOrdinalDate year dayOfYear)
          (picosecondsToDiffTime timeOfDayPico)
    {-# INLINE timeDecoder #-}
{-# INLINE decodeUTCTime #-}

--------------------------------------------------------------------------------
-- Wrapped CBORG decoders
--------------------------------------------------------------------------------

decodeTagMaybe :: Decoder s (Maybe Word64)
decodeTagMaybe =
  -- The same steps as Plain.decodeTagMaybe, through this module's primitives.
  peekTokenType >>= \case
    C.TypeTag -> Just . fromIntegral <$> decodeTag
    C.TypeTag64 -> Just <$> decodeTag64
    _ -> pure Nothing
{-# INLINE decodeTagMaybe #-}

allowTag :: Word -> Decoder s ()
allowTag tagExpected = do
  mTagReceived <- decodeTagMaybe
  forM_ mTagReceived $ \tagReceived ->
    unless (tagReceived == (fromIntegral tagExpected :: Word64)) $
      fail $
        "Expecteg tag " <> show tagExpected <> " but got tag " <> show tagReceived
{-# INLINE allowTag #-}

assertTag :: Word -> Decoder s ()
assertTag tagExpected = do
  -- The same steps as Plain.assertTag, through this module's primitives.
  tagReceived <-
    decodeTagMaybe >>= \case
      Just tag -> pure tag
      Nothing -> fail "Expected tag"
  unless (tagReceived == (fromIntegral tagExpected :: Word64)) $
    fail $
      "Expecteg tag " <> show tagExpected <> " but got tag " <> show tagReceived
{-# INLINE assertTag #-}

-- | Enforces that the input size is the same as the decoded one, failing in
--   case it's not
enforceSize :: Text.Text -> Int -> Decoder s ()
enforceSize lbl requestedSize = decodeListLen >>= matchSize lbl requestedSize
{-# INLINE enforceSize #-}

-- | Compare two sizes, failing if they are not equal
matchSize :: Text.Text -> Int -> Int -> Decoder s ()
matchSize lbl requestedSize actualSize =
  when (actualSize /= requestedSize) $
    cborError $
      DecoderErrorSizeMismatch
        lbl
        requestedSize
        actualSize
{-# INLINE matchSize #-}

decodeBool :: Decoder s Bool
decodeBool = directPrim C.decodeBool dBool
{-# INLINE decodeBool #-}

decodeBreakOr :: Decoder s Bool
decodeBreakOr = directPrim C.decodeBreakOr dBreakOr
{-# INLINE decodeBreakOr #-}

decodeByteArrayDefinite :: Decoder s ByteArray
decodeByteArrayDefinite = directPrim C.decodeByteArray dByteArray

decodeByteArray :: Decoder s ByteArray
decodeByteArray =
  ifDecoderVersionAtLeast
    (natVersion @12)
    decodeByteArrayDefOrIndef
    decodeByteArrayDefinite
{-# INLINE decodeByteArray #-}

decodeByteArrayCanonical :: Decoder s ByteArray
decodeByteArrayCanonical = fromPlainDecoder C.decodeByteArrayCanonical
{-# INLINE decodeByteArrayCanonical #-}

decodeBytesCanonical :: Decoder s BS.ByteString
decodeBytesCanonical = fromPlainDecoder C.decodeBytesCanonical
{-# INLINE decodeBytesCanonical #-}

decodeBytesIndef :: Decoder s ()
decodeBytesIndef = fromPlainDecoder C.decodeBytesIndef
{-# INLINE decodeBytesIndef #-}

decodeDouble :: Decoder s Double
decodeDouble = fromPlainDecoder C.decodeDouble
{-# INLINE decodeDouble #-}

decodeDoubleCanonical :: Decoder s Double
decodeDoubleCanonical = fromPlainDecoder C.decodeDoubleCanonical
{-# INLINE decodeDoubleCanonical #-}

decodeFloat :: Decoder s Float
decodeFloat = fromPlainDecoder C.decodeFloat
{-# INLINE decodeFloat #-}

decodeFloat16Canonical :: Decoder s Float
decodeFloat16Canonical = fromPlainDecoder C.decodeFloat16Canonical
{-# INLINE decodeFloat16Canonical #-}

decodeFloatCanonical :: Decoder s Float
decodeFloatCanonical = fromPlainDecoder C.decodeFloatCanonical
{-# INLINE decodeFloatCanonical #-}

decodeInt :: Decoder s Int
decodeInt = directPrim C.decodeInt dInt
{-# INLINE decodeInt #-}

decodeInt16 :: Decoder s Int16
decodeInt16 = fromPlainDecoder C.decodeInt16
{-# INLINE decodeInt16 #-}

decodeInt16Canonical :: Decoder s Int16
decodeInt16Canonical = fromPlainDecoder C.decodeInt16Canonical
{-# INLINE decodeInt16Canonical #-}

decodeInt32 :: Decoder s Int32
decodeInt32 = fromPlainDecoder C.decodeInt32
{-# INLINE decodeInt32 #-}

decodeInt32Canonical :: Decoder s Int32
decodeInt32Canonical = fromPlainDecoder C.decodeInt32Canonical
{-# INLINE decodeInt32Canonical #-}

decodeInt64 :: Decoder s Int64
decodeInt64 = directPrim C.decodeInt64 dInt64
{-# INLINE decodeInt64 #-}

decodeInt64Canonical :: Decoder s Int64
decodeInt64Canonical = fromPlainDecoder C.decodeInt64Canonical
{-# INLINE decodeInt64Canonical #-}

decodeInt8 :: Decoder s Int8
decodeInt8 = fromPlainDecoder C.decodeInt8
{-# INLINE decodeInt8 #-}

decodeInt8Canonical :: Decoder s Int8
decodeInt8Canonical = fromPlainDecoder C.decodeInt8Canonical
{-# INLINE decodeInt8Canonical #-}

decodeIntCanonical :: Decoder s Int
decodeIntCanonical = fromPlainDecoder C.decodeIntCanonical
{-# INLINE decodeIntCanonical #-}

decodeInteger :: Decoder s Integer
decodeInteger = directPrim C.decodeInteger dInteger
{-# INLINE decodeInteger #-}

decodeNatural :: Decoder s Natural
decodeNatural = do
  !n <- decodeInteger
  if n >= 0
    then return $! fromInteger n
    else cborError $ DecoderErrorCustom "Natural" "got a negative number"
{-# INLINE decodeNatural #-}

decodeIntegerCanonical :: Decoder s Integer
decodeIntegerCanonical = fromPlainDecoder C.decodeIntegerCanonical
{-# INLINE decodeIntegerCanonical #-}

decodeListLen :: Decoder s Int
decodeListLen = directPrim C.decodeListLen dListLen
{-# INLINE decodeListLen #-}

decodeListLenCanonical :: Decoder s Int
decodeListLenCanonical = fromPlainDecoder C.decodeListLenCanonical
{-# INLINE decodeListLenCanonical #-}

decodeListLenCanonicalOf :: Int -> Decoder s ()
decodeListLenCanonicalOf = fromPlainDecoder . C.decodeListLenCanonicalOf
{-# INLINE decodeListLenCanonicalOf #-}

decodeListLenIndef :: Decoder s ()
decodeListLenIndef = directPrim C.decodeListLenIndef dListLenIndef
{-# INLINE decodeListLenIndef #-}

decodeListLenOf :: Int -> Decoder s ()
decodeListLenOf n =
  -- cborg's decodeListLenOf is decodeListLen followed by an equality test.
  directPrim (C.decodeListLenOf n) $ \bs off -> case dListLen bs off of
    DOk off' len | len == n -> DOk off' ()
    _ -> DBail
{-# INLINE decodeListLenOf #-}

decodeListLenOrIndef :: Decoder s (Maybe Int)
decodeListLenOrIndef = directPrim C.decodeListLenOrIndef dListLenOrIndef
{-# INLINE decodeListLenOrIndef #-}

decodeMapLen :: Decoder s Int
decodeMapLen = directPrim C.decodeMapLen dMapLen
{-# INLINE decodeMapLen #-}

decodeMapLenCanonical :: Decoder s Int
decodeMapLenCanonical = fromPlainDecoder C.decodeMapLenCanonical
{-# INLINE decodeMapLenCanonical #-}

decodeMapLenIndef :: Decoder s ()
decodeMapLenIndef = directPrim C.decodeMapLenIndef dMapLenIndef
{-# INLINE decodeMapLenIndef #-}

decodeMapLenOrIndef :: Decoder s (Maybe Int)
decodeMapLenOrIndef = directPrim C.decodeMapLenOrIndef dMapLenOrIndef
{-# INLINE decodeMapLenOrIndef #-}

decodeNegWord :: Decoder s Word
decodeNegWord = fromPlainDecoder C.decodeNegWord
{-# INLINE decodeNegWord #-}

decodeNegWord64 :: Decoder s Word64
decodeNegWord64 = fromPlainDecoder C.decodeNegWord64
{-# INLINE decodeNegWord64 #-}

decodeNegWord64Canonical :: Decoder s Word64
decodeNegWord64Canonical = fromPlainDecoder C.decodeNegWord64Canonical
{-# INLINE decodeNegWord64Canonical #-}

decodeNegWordCanonical :: Decoder s Word
decodeNegWordCanonical = fromPlainDecoder C.decodeNegWordCanonical
{-# INLINE decodeNegWordCanonical #-}

decodeNull :: Decoder s ()
decodeNull = directPrim C.decodeNull dNull
{-# INLINE decodeNull #-}

decodeSequenceLenIndef :: (r -> a -> r) -> r -> (r -> b) -> Decoder s a -> Decoder s b
decodeSequenceLenIndef f z g dec =
  Decoder
    (\bsl v -> C.decodeSequenceLenIndef f z g (runDecoder dec bsl v))
    -- cborg's decodeSequenceLenIndef, step for step, through this module's primitives
    (runDirect (go z))
  where
    go !acc =
      decodeBreakOr >>= \stop ->
        if stop
          then pure $! g acc
          else dec >>= \ !x -> go (f acc x)
{-# INLINE decodeSequenceLenIndef #-}

decodeSequenceLenN :: (r -> a -> r) -> r -> (r -> b) -> Int -> Decoder s a -> Decoder s b
decodeSequenceLenN f z g c dec =
  Decoder
    (\bsl v -> C.decodeSequenceLenN f z g c (runDecoder dec bsl v))
    -- cborg's decodeSequenceLenN, step for step
    (runDirect (go z c))
  where
    go !acc 0 = pure $! g acc
    go !acc n = dec >>= \ !x -> go (f acc x) (n - 1 :: Int)
{-# INLINE decodeSequenceLenN #-}

decodeSimple :: Decoder s Word8
decodeSimple = fromPlainDecoder C.decodeSimple
{-# INLINE decodeSimple #-}

decodeSimpleCanonical :: Decoder s Word8
decodeSimpleCanonical = fromPlainDecoder C.decodeSimpleCanonical
{-# INLINE decodeSimpleCanonical #-}

decodeStringDefinite :: Decoder s Text.Text
decodeStringDefinite = directPrim C.decodeString dString
{-# INLINE decodeStringDefinite #-}

decodeString :: Decoder s Text.Text
decodeString =
  ifDecoderVersionAtLeast
    (natVersion @12)
    decodeStringDefOrIndef
    decodeStringDefinite
{-# INLINE decodeString #-}

decodeStringCanonical :: Decoder s Text.Text
decodeStringCanonical = fromPlainDecoder C.decodeStringCanonical
{-# INLINE decodeStringCanonical #-}

decodeStringIndef :: Decoder s ()
decodeStringIndef = fromPlainDecoder C.decodeStringIndef
{-# INLINE decodeStringIndef #-}

-- | Loop over chunks of an indefinite-length text string until a break token is
-- encountered, concatenating the chunks in order.
decodeStringIndefLen :: Decoder s Text.Text
decodeStringIndefLen = do
  decodeStringIndef
  decodeSequenceLenIndef
    (\b t -> b <> B.fromText t)
    mempty
    (LT.toStrict . B.toLazyText)
    decodeStringDefinite
{-# INLINE decodeStringIndefLen #-}

-- | Decode a text string that may be either definite-length or
-- indefinite-length encoded.
decodeStringDefOrIndef :: Decoder s Text.Text
decodeStringDefOrIndef =
  peekTokenType >>= \case
    C.TypeStringIndef -> decodeStringIndefLen
    _ -> decodeStringDefinite
{-# INLINE decodeStringDefOrIndef #-}

-- | Loop over chunks of an indefinite-length byte string until a break token
-- is encountered, concatenating the chunks in order.
decodeBytesIndefLen :: Decoder s BS.ByteString
decodeBytesIndefLen = do
  decodeBytesIndef
  decodeSequenceLenIndef
    (\b bs -> b <> BSB.byteString bs)
    mempty
    (LBS.toStrict . BSB.toLazyByteString)
    decodeBytesDefinite
{-# INLINE decodeBytesIndefLen #-}

-- | Decode a byte string that may be either definite-length or
-- indefinite-length encoded.
decodeBytesDefOrIndef :: Decoder s BS.ByteString
decodeBytesDefOrIndef =
  peekTokenType >>= \case
    C.TypeBytesIndef -> decodeBytesIndefLen
    _ -> decodeBytesDefinite
{-# INLINE decodeBytesDefOrIndef #-}

-- | 'decodeBytesIndefLen' specialised to the cborg 'ByteArray' type.
decodeByteArrayIndefLen :: Decoder s ByteArray
decodeByteArrayIndefLen = do
  decodeBytesIndef
  decodeSequenceLenIndef
    (flip (:))
    []
    (BA . mconcat . reverse . map unBA)
    decodeByteArrayDefinite
{-# INLINE decodeByteArrayIndefLen #-}

-- | 'decodeBytesDefOrIndef' specialised to the cborg 'ByteArray' type.
decodeByteArrayDefOrIndef :: Decoder s ByteArray
decodeByteArrayDefOrIndef =
  peekTokenType >>= \case
    C.TypeBytesIndef -> decodeByteArrayIndefLen
    _ -> decodeByteArrayDefinite
{-# INLINE decodeByteArrayDefOrIndef #-}

decodeTag :: Decoder s Word
decodeTag = directPrim C.decodeTag dTag
{-# INLINE decodeTag #-}

decodeTag64 :: Decoder s Word64
decodeTag64 = directPrim C.decodeTag64 dTag64
{-# INLINE decodeTag64 #-}

decodeTag64Canonical :: Decoder s Word64
decodeTag64Canonical = fromPlainDecoder C.decodeTag64Canonical
{-# INLINE decodeTag64Canonical #-}

decodeTagCanonical :: Decoder s Word
decodeTagCanonical = fromPlainDecoder C.decodeTagCanonical
{-# INLINE decodeTagCanonical #-}

decodeUtf8ByteArray :: Decoder s ByteArray
decodeUtf8ByteArray = fromPlainDecoder C.decodeUtf8ByteArray
{-# INLINE decodeUtf8ByteArray #-}

decodeUtf8ByteArrayCanonical :: Decoder s ByteArray
decodeUtf8ByteArrayCanonical = fromPlainDecoder C.decodeUtf8ByteArrayCanonical
{-# INLINE decodeUtf8ByteArrayCanonical #-}

decodeWithByteSpan :: Decoder s a -> Decoder s (a, C.ByteOffset, C.ByteOffset)
decodeWithByteSpan d =
  Decoder
    (\bsl v -> C.decodeWithByteSpan (runDecoder d bsl v))
    -- cborg's decodeWithByteSpan reports the offsets before and after the value;
    -- in the direct form those are the offsets into the chunk.
    ( \v bs off ->
        runDirect d v bs off >>= \case
          DOk off' x ->
            let !start = fromIntegral off
                !end = fromIntegral off'
             in pure (DOk off' (x, start, end))
          DBail -> pure DBail
    )
{-# INLINE decodeWithByteSpan #-}

decodeWord :: Decoder s Word
decodeWord = directPrim C.decodeWord dWord
{-# INLINE decodeWord #-}

decodeWord16 :: Decoder s Word16
decodeWord16 = directPrim C.decodeWord16 dWord16
{-# INLINE decodeWord16 #-}

decodeWord16Canonical :: Decoder s Word16
decodeWord16Canonical = fromPlainDecoder C.decodeWord16Canonical
{-# INLINE decodeWord16Canonical #-}

decodeWord32 :: Decoder s Word32
decodeWord32 = directPrim C.decodeWord32 dWord32
{-# INLINE decodeWord32 #-}

decodeWord32Canonical :: Decoder s Word32
decodeWord32Canonical = fromPlainDecoder C.decodeWord32Canonical
{-# INLINE decodeWord32Canonical #-}

decodeWord64 :: Decoder s Word64
decodeWord64 = directPrim C.decodeWord64 dWord64
{-# INLINE decodeWord64 #-}

decodeWord64Canonical :: Decoder s Word64
decodeWord64Canonical = fromPlainDecoder C.decodeWord64Canonical
{-# INLINE decodeWord64Canonical #-}

decodeWord8 :: Decoder s Word8
decodeWord8 = directPrim C.decodeWord8 dWord8
{-# INLINE decodeWord8 #-}

decodeWord8Canonical :: Decoder s Word8
decodeWord8Canonical = fromPlainDecoder C.decodeWord8Canonical
{-# INLINE decodeWord8Canonical #-}

decodeWordCanonical :: Decoder s Word
decodeWordCanonical = fromPlainDecoder C.decodeWordCanonical
{-# INLINE decodeWordCanonical #-}

decodeWordCanonicalOf :: Word -> Decoder s ()
decodeWordCanonicalOf = fromPlainDecoder . C.decodeWordCanonicalOf
{-# INLINE decodeWordCanonicalOf #-}

decodeWordOf :: Word -> Decoder s ()
decodeWordOf = fromPlainDecoder . C.decodeWordOf
{-# INLINE decodeWordOf #-}

decodeTerm :: Decoder s C.Term
decodeTerm = fromPlainDecoder C.decodeTerm
{-# INLINE decodeTerm #-}

peekAvailable :: Decoder s Int
peekAvailable = Decoder (\_ _ -> C.peekAvailable) (\_ _ _ -> pure DBail)
{-# INLINE peekAvailable #-}

peekByteOffset :: Decoder s C.ByteOffset
peekByteOffset =
  Decoder
    (\_ _ -> C.peekByteOffset)
    (\_ _ off -> let !o = fromIntegral off in pure (DOk off o))
{-# INLINE peekByteOffset #-}

peekTokenType :: Decoder s C.TokenType
peekTokenType = directPeek C.peekTokenType dTokenType
{-# INLINE peekTokenType #-}

liftST :: ST s a -> Decoder s a
liftST st = Decoder (\_ _ -> C.liftST st) (\_ _ off -> st >>= \x -> pure (DOk off x))
{-# INLINE liftST #-}

--------------------------------------------------------------------------------
-- Direct primitives (DIRECT-PRIMS-BEGIN)
--------------------------------------------------------------------------------
--
-- Direct readings of single CBOR tokens out of one strict chunk, for
-- `directPrim`. Each answers `DOk` only where the cborg-0.2.10.0 primitive
-- named in its comment, run at the same position with the whole token in the
-- chunk, succeeds with the same value and length; everything else is `DBail`,
-- which hands that token to cborg. Line references are to cborg-0.2.10.0's
-- Codec/CBOR/Read.hs and Codec/CBOR/Decoding.hs.

------------------------------------------------------------------------------
-- Helpers

-- | True on 64-bit targets (Int and Word are 64 bits). Constant-folded.
dIs64 :: Bool
dIs64 = finiteBitSize (0 :: Int) == 64 && finiteBitSize (0 :: Word) == 64
{-# INLINE dIs64 #-}

-- | Is @off@ a valid index of a byte in @bs@ (and are we on 64-bit)?
dOkOff :: ByteString -> Int -> Bool
dOkOff bs off = dIs64 && off >= 0 && off < BS.length bs
{-# INLINE dOkOff #-}

-- | 2^63: arguments at or above this do not fit a non-negative Int; cborg's
-- word64ToInt returns Nothing for them.
dTwoP63 :: Word64
dTwoP63 = 0x8000000000000000
{-# INLINE dTwoP63 #-}

-- | Decode the argument of the head byte at offset @p@ (precondition:
-- @0 <= p < BS.length bs@), given its additional-info bits @ai@.
-- Returns the offset just after the head and the argument. Bails on
-- additional info 28..31 and when the head is truncated (cborg's
-- go_fast_end tokenSize check, Read.hs:747-750, would leave the token to the
-- slow path, which needs more input). Big-endian, any width.
dArg :: ByteString -> Int -> Word8 -> DRes Word64
dArg bs p ai
  | ai < 24 = DOk (p + 1) (fromIntegral ai)
  | ai == 24 =
      if BS.length bs - p >= 2
        then DOk (p + 2) (fromIntegral (BS.index bs (p + 1)))
        else DBail
  | otherwise = dArgWide bs p ai
{-# INLINE dArg #-}

-- | The 2, 4 and 8 byte arguments (additional info 25, 26, 27) of `dArg`, kept out
-- of line so that inlining the token readers copies only the short cases.
dArgWide :: ByteString -> Int -> Word8 -> DRes Word64
dArgWide bs p ai
  | ai == 25 = if rest >= 3 then DOk (p + 3) ((b 1 `shiftL` 8) .|. b 2) else DBail
  | ai == 26 =
      if rest >= 5
        then
          DOk
            (p + 5)
            ((b 1 `shiftL` 24) .|. (b 2 `shiftL` 16) .|. (b 3 `shiftL` 8) .|. b 4)
        else DBail
  | ai == 27 =
      if rest >= 9
        then
          DOk
            (p + 9)
            ( (b 1 `shiftL` 56)
                .|. (b 2 `shiftL` 48)
                .|. (b 3 `shiftL` 40)
                .|. (b 4 `shiftL` 32)
                .|. (b 5 `shiftL` 24)
                .|. (b 6 `shiftL` 16)
                .|. (b 7 `shiftL` 8)
                .|. b 8
            )
        else DBail
  | otherwise = DBail
  where
    rest = BS.length bs - p -- >= 1, no overflow (0 <= p < len)
    b :: Int -> Word64
    b i = fromIntegral (BS.index bs (p + i))
{-# NOINLINE dArgWide #-}

-- | Argument of a head whose major type must be @mt@ (0..7).
dArgOfMajor :: Word8 -> ByteString -> Int -> DRes Word64
dArgOfMajor mt bs off
  | not (dOkOff bs off) = DBail
  | otherwise =
      let !h = BS.index bs off
      in if h `shiftR` 5 == mt then dArg bs off (h .&. 0x1f) else DBail
{-# INLINE dArgOfMajor #-}

------------------------------------------------------------------------------
-- Unsigned integers

-- | decodeWord = ConsumeWord (Decoding.hs:392-393).
-- DOk: heads 0x00-0x1b (major 0), any width incl. over-long, any 64-bit
-- value. Read.hs:1595-1633 tryConsumeWord: 0x00-0x17 literal, 0x18/0x19/0x1a
-- size 2/3/5, 0x1b size 9 with word64ToWord (total on ARCH_64bit,
-- Read.hs:1625-1626). go_fast Read.hs:271-274, go_fast_end Read.hs:752-755.
-- Bail: every other head (cborg: DecodeFailure -> "expected word"; note
-- major 1 is NOT accepted by decodeWord), truncated head, bad offset.
{-# INLINE dWord #-}
dWord :: ByteString -> Int -> DRes Word
dWord bs off = case dArgOfMajor 0 bs off of
  DOk o w -> DOk o (fromIntegral w)
  DBail   -> DBail

-- | decodeWord8 = ConsumeWord8 (Decoding.hs:399-400).
-- DOk: major-0 heads (any width) whose value <= 0xff. go_fast Read.hs:276-282
-- (gtWord# w# 0xff## -> 0# continues), go_fast_end Read.hs:757-763.
-- Bail: value > 0xff (cborg: "expected word8"), non-major-0, truncation.
{-# INLINE dWord8 #-}
dWord8 :: ByteString -> Int -> DRes Word8
dWord8 bs off = case dArgOfMajor 0 bs off of
  DOk o w | w <= 0xff -> DOk o (fromIntegral w)
  _                   -> DBail

-- | decodeWord16 = ConsumeWord16 (Decoding.hs:406-407).
-- DOk: major-0 heads (any width) with value <= 0xffff. go_fast
-- Read.hs:284-290, go_fast_end Read.hs:765-771.
-- Bail: value > 0xffff (cborg fails), otherwise as dWord.
{-# INLINE dWord16 #-}
dWord16 :: ByteString -> Int -> DRes Word16
dWord16 bs off = case dArgOfMajor 0 bs off of
  DOk o w | w <= 0xffff -> DOk o (fromIntegral w)
  _                     -> DBail

-- | decodeWord32 = ConsumeWord32 (Decoding.hs:413-414).
-- DOk: major-0 heads (any width) with value <= 0xffffffff. go_fast
-- Read.hs:292-301 (64-bit branch: gtWord# w# 0xffffffff##), go_fast_end
-- Read.hs:773-783.  Bail: value > 0xffffffff (cborg fails), otherwise as dWord.
{-# INLINE dWord32 #-}
dWord32 :: ByteString -> Int -> DRes Word32
dWord32 bs off = case dArgOfMajor 0 bs off of
  DOk o w | w <= 0xffffffff -> DOk o (fromIntegral w)
  _                         -> DBail

-- | decodeWord64: on ARCH_64bit it is ConsumeWord (Decoding.hs:420-427), so
-- exactly dWord's acceptance: all major-0 heads, any width, any value.
{-# INLINE dWord64 #-}
dWord64 :: ByteString -> Int -> DRes Word64
dWord64 = dArgOfMajor 0

------------------------------------------------------------------------------
-- Signed integers

-- | decodeInt = ConsumeInt (Decoding.hs:451-452); tryConsumeInt Read.hs:1677-1755.
-- DOk:
--   * 0x00-0x17 (value = ai), 0x18/0x19/0x1a (any value; on 64-bit
--     word32ToInt is total, Read.hs:1706-1707), 0x1b with value < 2^63
--     (word64ToInt Just, Read.hs:1713-1715).
--   * 0x20-0x37 (value -1-ai), 0x38/0x39/0x3a (-1-n, Read.hs:1742-1745),
--     0x3b with n < 2^63 -> -1-n >= -2^63 (Read.hs:1751-1753).
--   go_fast Read.hs:309-312, go_fast_end Read.hs:790-793.
-- Bail: 0x1b / 0x3b with n >= 2^63 (cborg: word64ToInt Nothing ->
--   DecodeFailure -> "expected int"); every other major; ai 28-31; truncation.
{-# INLINE dInt #-}
dInt :: ByteString -> Int -> DRes Int
dInt bs off
  | not (dOkOff bs off) = DBail
  | otherwise =
      let !h = BS.index bs off
          !mt = h `shiftR` 5
      in if mt == 0 then
           case dArg bs off (h .&. 0x1f) of
             DOk o w | w < dTwoP63 -> DOk o (fromIntegral w)
             _                    -> DBail
         else if mt == 1 then
           case dArg bs off (h .&. 0x1f) of
             DOk o w | w < dTwoP63 -> DOk o (-1 - fromIntegral w)
             _                    -> DBail
         else DBail

-- | decodeInt64: on ARCH_64bit it is ConsumeInt (Decoding.hs:479-486), so
-- exactly dInt's acceptance and value (toInt64 is the identity on 64-bit).
{-# INLINE dInt64 #-}
dInt64 :: ByteString -> Int -> DRes Int64
dInt64 bs off = case dInt bs off of
  DOk o n -> DOk o (fromIntegral n)
  DBail   -> DBail

-- | decodeInteger = ConsumeInteger (Decoding.hs:590-591);
-- tryConsumeInteger Read.hs:1758-1850.
-- DOk: 0x00-0x1b -> toInteger n; 0x20-0x3b -> -1 - toInteger n, for ALL
--   64-bit n, any width (BigIntToken; the canonical flag is ignored by
--   ConsumeInteger). go_fast Read.hs:541-544 (any BigIntToken), go_fast_end
--   Read.hs:1030-1032.
-- Bail (by choice): 0xc2 / 0xc3 bignum tags (cborg handles them, possibly
--   across chunks: readBigUInt/readBigNInt) -- we never decode them here.
-- Bail: everything else (cborg: DecodeFailure "expected integer"), truncation.
{-# INLINE dInteger #-}
dInteger :: ByteString -> Int -> DRes Integer
dInteger bs off
  | not (dOkOff bs off) = DBail
  | otherwise =
      let !h = BS.index bs off
          !mt = h `shiftR` 5
      in if mt == 0 then
           case dArg bs off (h .&. 0x1f) of
             DOk o w -> DOk o (toInteger w)
             DBail   -> DBail
         else if mt == 1 then
           case dArg bs off (h .&. 0x1f) of
             DOk o w -> DOk o (-1 - toInteger w)
             DBail   -> DBail
         else DBail

------------------------------------------------------------------------------
-- Lists and maps

-- | Major type @mt@ (4 or 5) definite length that must fit a non-negative Int.
dLenOfMajor :: Word8 -> ByteString -> Int -> DRes Int
dLenOfMajor mt bs off = case dArgOfMajor mt bs off of
  DOk o w | w < dTwoP63 -> DOk o (fromIntegral w)
  _                    -> DBail
{-# INLINE dLenOfMajor #-}

-- | decodeListLen = ConsumeListLen (Decoding.hs:701-702);
-- tryConsumeListLen Read.hs:1925-1965.
-- DOk: 0x80-0x97 (len = ai), 0x98/0x99/0x9a any value (64-bit
--   word32ToInt total, Read.hs:1954-1955), 0x9b with n < 2^63
--   (word64ToInt Just, Read.hs:1961-1963). Any width (over-long accepted).
--   go_fast Read.hs:342-345, go_fast_end Read.hs:823-826.
-- Bail: 0x9f indefinite (no case in tryConsumeListLen -> "expected list
--   len"), 0x9b with n >= 2^63 (word64ToInt Nothing -> DecodeFailure),
--   0x9c-0x9e, other majors, truncation.
{-# INLINE dListLen #-}
dListLen :: ByteString -> Int -> DRes Int
dListLen = dLenOfMajor 4

-- | decodeMapLen = ConsumeMapLen (Decoding.hs:723-724);
-- tryConsumeMapLen Read.hs:1968-2008. Same shape as dListLen on 0xa0-0xbb.
-- go_fast Read.hs:347-350, go_fast_end Read.hs:828-831.
-- Bail: 0xbf, 0xbb with n >= 2^63, 0xbc-0xbe, other majors, truncation.
{-# INLINE dMapLen #-}
dMapLen :: ByteString -> Int -> DRes Int
dMapLen = dLenOfMajor 5

-- | decodeListLenOrIndef (Decoding.hs:914-920): ConsumeListLenOrIndef, then
-- n >= 0 -> Just n, else Nothing. tryConsumeListLenOrIndef Read.hs:2025-2067:
-- definite heads as dListLen (values are always >= 0, since 0x9b demands
-- n < 2^63), 0x9f -> (-1) -> Nothing (Read.hs:2065), size 1.
-- go_fast Read.hs:688-691, go_fast_end Read.hs:1197-1200.
-- Bail: as dListLen except 0x9f.
{-# INLINE dListLenOrIndef #-}
dListLenOrIndef :: ByteString -> Int -> DRes (Maybe Int)
dListLenOrIndef bs off
  | dOkOff bs off && BS.index bs off == 0x9f = DOk (off + 1) Nothing
  | otherwise = case dLenOfMajor 4 bs off of
      DOk o n -> n `seq` DOk o (Just n) -- cborg's Just holds an evaluated length
      DBail   -> DBail

-- | decodeMapLenOrIndef (Decoding.hs:928-934); tryConsumeMapLenOrIndef
-- Read.hs:2070-2112, 0xbf -> Nothing (Read.hs:2110).
-- go_fast Read.hs:693-696, go_fast_end Read.hs:1202-1205.
{-# INLINE dMapLenOrIndef #-}
dMapLenOrIndef :: ByteString -> Int -> DRes (Maybe Int)
dMapLenOrIndef bs off
  | dOkOff bs off && BS.index bs off == 0xbf = DOk (off + 1) Nothing
  | otherwise = case dLenOfMajor 5 bs off of
      DOk o n -> n `seq` DOk o (Just n) -- cborg's Just holds an evaluated length
      DBail   -> DBail

-- | decodeListLenIndef (Decoding.hs:716-717): only 0x9f, size 1
-- (tryConsumeListLenIndef Read.hs:2011-2015; go_fast Read.hs:673-676,
-- go_fast_end Read.hs:1182-1185). Anything else: cborg fails -> DBail.
dListLenIndef :: ByteString -> Int -> DRes ()
dListLenIndef bs off
  | dOkOff bs off && BS.index bs off == 0x9f = DOk (off + 1) ()
  | otherwise = DBail
{-# INLINE dListLenIndef #-}

-- | decodeMapLenIndef (Decoding.hs:738-739): only 0xbf, size 1
-- (Read.hs:2018-2022; go_fast Read.hs:678-681, go_fast_end Read.hs:1187-1190).
dMapLenIndef :: ByteString -> Int -> DRes ()
dMapLenIndef bs off
  | dOkOff bs off && BS.index bs off == 0xbf = DOk (off + 1) ()
  | otherwise = DBail
{-# INLINE dMapLenIndef #-}

-- | decodeBreakOr (Decoding.hs:941-942). tryConsumeBreakOr Read.hs:2483-2486:
-- 0xff -> DOk +1 True; any other byte -> False consuming nothing
-- (go_fast Read.hs:698-701: DecodeFailure -> k False >>= go_fast bs;
-- go_fast_end Read.hs:735-738 likewise). Note any non-0xff byte, even an
-- invalid head, gives False: cborg does not inspect it further here.
-- Bail: no byte available (go_fast_end Read.hs:733: BS.null -> slow path,
-- which needs more input), bad offset.
dBreakOr :: ByteString -> Int -> DRes Bool
dBreakOr bs off
  | not (dOkOff bs off)        = DBail
  | BS.index bs off == 0xff   = DOk (off + 1) True
  | otherwise                 = DOk off False
{-# INLINE dBreakOr #-}

------------------------------------------------------------------------------
-- Tags

-- | decodeTag = ConsumeTag (Decoding.hs:745-746); tryConsumeTag
-- Read.hs:2115-2154. DOk: 0xc0-0xdb, any width, any 64-bit value
-- (0xdb uses word64ToWord, total on ARCH_64bit, Read.hs:2147-2148).
-- Note 0xc2/0xc3 ARE accepted as plain tags 2/3 by decodeTag.
-- go_fast Read.hs:352-355, go_fast_end Read.hs:833-836.
-- Bail: other heads (incl. 0xdc-0xdf), truncation.
{-# INLINE dTag #-}
dTag :: ByteString -> Int -> DRes Word
dTag bs off = case dArgOfMajor 6 bs off of
  DOk o w -> DOk o (fromIntegral w)
  DBail   -> DBail

-- | decodeTag64: on ARCH_64bit it is ConsumeTag (Decoding.hs:752-765), so
-- exactly dTag's acceptance, value widened to Word64.
{-# INLINE dTag64 #-}
dTag64 :: ByteString -> Int -> DRes Word64
dTag64 = dArgOfMajor 6

------------------------------------------------------------------------------
-- Simple values

-- | decodeNull (Decoding.hs:804-805): only 0xf6, size 1 (tryConsumeNull
-- Read.hs:2476-2480; go_fast Read.hs:683-686, go_fast_end Read.hs:1192-1195).
dNull :: ByteString -> Int -> DRes ()
dNull bs off
  | dOkOff bs off && BS.index bs off == 0xf6 = DOk (off + 1) ()
  | otherwise = DBail
{-# INLINE dNull #-}

-- | decodeBool (Decoding.hs:797-798): 0xf4 -> False, 0xf5 -> True, size 1
-- (tryConsumeBool Read.hs:2421-2425; go_fast Read.hs:587-590, go_fast_end
-- Read.hs:1081-1084). Everything else (incl. 0xf8 0x14 simple 20): DBail.
dBool :: ByteString -> Int -> DRes Bool
dBool bs off
  | not (dOkOff bs off) = DBail
  | otherwise = case BS.index bs off of
      0xf4 -> DOk (off + 1) False
      0xf5 -> DOk (off + 1) True
      _    -> DBail
{-# INLINE dBool #-}

------------------------------------------------------------------------------
-- Byte and text strings

-- | Definite-length payload of major @mt@ (2 or 3), wholly in the buffer.
dPayloadOfMajor :: Word8 -> ByteString -> Int -> DRes ByteString
dPayloadOfMajor mt bs off = case dArgOfMajor mt bs off of
  DOk o w
    | w < dTwoP63
    , let !n = fromIntegral w :: Int
    , n <= BS.length bs - o          -- no overflow: o <= length
    -> DOk (o + n) (BS.take n (BS.drop o bs))
  _ -> DBail
{-# INLINE dPayloadOfMajor #-}

-- | decodeBytes = ConsumeBytes (Decoding.hs:611-612); tryConsumeBytes
-- Read.hs:1853-1886 with readBytesSmall/8/16/32/64 Read.hs:2489-2565.
-- DOk: 0x40-0x5b (any width of length head) when the length n satisfies
--   n <= bytes remaining after the head ("Fits", Read.hs:2492-2494,
--   2510-2512, 2523-2525, 2535-2543 (64-bit: word32ToInt total), 2554-2558
--   (0x5b requires word64ToInt Just, i.e. n < 2^63)); consumes head + n;
--   value = the n payload bytes (zero-length -> empty).
--   go_fast Read.hs:556-559, go_fast_end Read.hs:1049-1052 (Fits branch).
-- Bail: TooLong (payload not wholly in buffer: cborg goes to the slow path,
--   needs more input); 0x5b with n >= 2^63 (DecodeFailure, Read.hs:2563);
--   0x5f indefinite and 0x5c-0x5e (DecodeFailure); other majors; truncated
--   head.
{-# INLINE dBytes #-}
dBytes :: ByteString -> Int -> DRes ByteString
dBytes = dPayloadOfMajor 2

-- | decodeString = ConsumeString (Decoding.hs:655-656); tryConsumeString
-- Read.hs:1889-1922 (same readBytes* helpers as bytes, on 0x60-0x7b).
-- DOk: as dBytes on 0x60-0x7b AND Data.Text.Encoding.decodeUtf8' of the
--   payload is Right t; value t. cborg calls exactly T.decodeUtf8' on the
--   Fits payload: go_fast Read.hs:570-574, go_fast_end Read.hs:1064-1068.
-- Bail: invalid UTF-8 (cborg: SlowFail "invalid UTF8"), TooLong, 0x7f
--   indefinite, 0x7c-0x7e, 0x7b with n >= 2^63, other majors, truncation.
{-# INLINE dString #-}
dString :: ByteString -> Int -> DRes Text.Text
dString bs off = case dPayloadOfMajor 3 bs off of
  DOk o p -> case Text.decodeUtf8' p of
    Right t -> DOk o t
    Left _  -> DBail
  DBail   -> DBail

------------------------------------------------------------------------------
-- Token type peek

-- | peekTokenType = PeekTokenType (Decoding.hs:952-953). Consumes nothing.
-- cborg: go_fast Read.hs:703-706 / go_fast_end Read.hs:740-743 look up
-- decodeTokenTypeTable (Read.hs:1461-1503) on the head byte ONLY; a byte
-- is required (go_fast_end Read.hs:733: empty -> slow path).
-- DOk: every head byte whose head (tokenSize bytes: 1, or 2/3/5/9 for ai
--   24/25/26/27) is wholly present; value per the table:
--     mt 0: ai 0-26 C.TypeUInt, 27 C.TypeUInt64, 28-31 C.TypeInvalid
--     mt 1: ai 0-26 C.TypeNInt, 27 C.TypeNInt64, 28-31 C.TypeInvalid
--     mt 2: ai 0-27 C.TypeBytes, 28-30 C.TypeInvalid, 31 C.TypeBytesIndef
--     mt 3: ai 0-27 C.TypeString, 28-30 C.TypeInvalid, 31 C.TypeStringIndef
--     mt 4: ai 0-26 C.TypeListLen, 27 C.TypeListLen64, 28-30 Invalid, 31 Indef
--     mt 5: ai 0-26 C.TypeMapLen, 27 C.TypeMapLen64, 28-30 Invalid, 31 Indef
--     mt 6: ai 2,3 C.TypeInteger; 0,1,4-26 C.TypeTag; 27 C.TypeTag64; 28-31 Invalid
--     mt 7: ai 0-19 C.TypeSimple, 20/21 C.TypeBool, 22 C.TypeNull, 23/24
--           C.TypeSimple, 25/26/27 C.TypeFloat16/32/64, 28-30 Invalid, 31 Break
-- Bail: no byte (end of input / bad offset) -- cborg's slow path would need
--   more input. Also declined here when the head's argument bytes are
--   truncated; `directPrim` then asks cborg, which answers from the first byte.
{-# INLINE dTokenType #-}
dTokenType :: ByteString -> Int -> DRes C.TokenType
dTokenType bs off
  | not (dOkOff bs off) = DBail
  | otherwise =
      let !h   = BS.index bs off
          !mt  = h `shiftR` 5
          !ai  = h .&. 0x1f
          !need | ai < 24 || ai > 27 = 1
                | ai == 24  = 2
                | ai == 25  = 3
                | ai == 26  = 5
                | otherwise = 9 :: Int
      in if BS.length bs - off < need then DBail
         else DOk off (dClassify mt ai)

{-# INLINE dClassify #-}
dClassify :: Word8 -> Word8 -> C.TokenType
dClassify mt ai
  | ai >= 28 && ai <= 30 = C.TypeInvalid
  | otherwise = case mt of
      0 -> intLike C.TypeUInt C.TypeUInt64
      1 -> intLike C.TypeNInt C.TypeNInt64
      2 -> if ai == 31 then C.TypeBytesIndef else C.TypeBytes
      3 -> if ai == 31 then C.TypeStringIndef else C.TypeString
      4 -> lenLike C.TypeListLen C.TypeListLen64 C.TypeListLenIndef
      5 -> lenLike C.TypeMapLen C.TypeMapLen64 C.TypeMapLenIndef
      6 | ai == 2 || ai == 3 -> C.TypeInteger
        | ai == 31  -> C.TypeInvalid
        | ai == 27  -> C.TypeTag64
        | otherwise -> C.TypeTag
      _ | ai <= 19  -> C.TypeSimple
        | ai <= 21  -> C.TypeBool
        | ai == 22  -> C.TypeNull
        | ai <= 24  -> C.TypeSimple
        | ai == 25  -> C.TypeFloat16
        | ai == 26  -> C.TypeFloat32
        | ai == 27  -> C.TypeFloat64
        | otherwise -> C.TypeBreak
  where
    intLike small big
      | ai == 31  = C.TypeInvalid
      | ai == 27  = big
      | otherwise = small
    lenLike small big indef
      | ai == 31  = indef
      | ai == 27  = big
      | otherwise = small

-- | decodeByteArray = ConsumeByteArray: cborg reads the token with the same
-- tryConsumeBytes as decodeBytes (go_fast Read.hs:563-566) and copies the payload
-- into a fresh byte array, as `toShort` does here.
{-# INLINE dByteArray #-}
dByteArray :: ByteString -> Int -> DRes ByteArray
dByteArray bs off = case dBytes bs off of
  DOk o payload -> case toShort payload of
    SBS ba -> DOk o (BA (Prim.ByteArray ba))
  DBail -> DBail

-- DIRECT-PRIMS-END
