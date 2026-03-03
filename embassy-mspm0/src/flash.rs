use cortex_m::{asm, interrupt};
use embassy_hal_internal::Peri;
use crate::peripherals::FLASHCTL;
use mspm0_metapac::flashctl::vals;
use embedded_storage::nor_flash::{
    ErrorType, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash, check_erase, check_read,
    check_write,
};

const WORD_SIZE: usize = 8;

pub struct Flash<'d> {
    _peripheral: Peri<'d, FLASHCTL>,
}

impl<'d> Flash<'d> {
    pub fn new(peripheral: Peri<'d, FLASHCTL>) -> Self {
        Self {
            _peripheral: peripheral,
        }
    }

    fn unprotect_sector(&mut self, _offset: u32) {
        // TODO: do better, we just unprotect everything here
        regs().cmdweprota().write(|_| {});
        regs().cmdweprotb().write(|_| {});
        regs().cmdweprotc().write(|_| {});
    }

    fn write_word(&mut self, offset: u32, word: &[u8]) -> Result<(), Error> {
        self.clr_stat();
        regs().cmdtype().write(|w| {
            w.set_command(vals::Command::PROGRAM);
            w.set_size(vals::Size::ONEWORD);
        });
        // reset
        regs().cmdctl().write(|_| {});
        regs().cmdaddr().write_value(offset);
        regs().cmdbyten().write(|w| {
            for i in 0..WORD_SIZE {
                w.set_data(i, true);
            }
            w.set_ecc(true);
        });

        regs()
            .cmddata(0)
            .write_value(u32::from_le_bytes(word[0..4].try_into().unwrap()));
        regs()
            .cmddata(1)
            .write_value(u32::from_le_bytes(word[4..8].try_into().unwrap()));

        self.unprotect_sector(offset);

        self.do_cmd()
    }

    fn erase_sector(&mut self, offset: u32) -> Result<(), Error> {
        self.clr_stat();
        regs().cmdtype().write(|w| {
            w.set_command(vals::Command::ERASE);
            w.set_size(vals::Size::SECTOR);
        });
        // reset
        regs().cmdctl().write(|_| {});
        regs().cmdaddr().write_value(offset);

        self.unprotect_sector(offset);

        self.do_cmd()
    }

    fn clr_stat(&mut self) {
        regs()
            .cmdtype()
            .write(|w| w.set_command(vals::Command::CLEARSTATUS));
        while regs().statcmd().read().inprogress() {}
    }

    fn do_cmd(&mut self) -> Result<(), Error> {
        interrupt::free(|_| self.do_cmd_inner())
    }

    // TODO: link section correctly
    #[cfg_attr(feature="flash-ram", unsafe(link_section = ".data"), inline(never))]
    #[cfg_attr(not(feature="flash-ram"), inline(always))]
    fn do_cmd_inner(&mut self) -> Result<(), Error> {
        regs().cmdexec().write(|w| w.set_val(true));

        let ret = loop {
            let stat = regs().statcmd().read();
            if !stat.done() {
                continue;
            }

            break if stat.failweprot() {
                Err(Error::WriteProtected)
            } else if stat.faililladdr() {
                Err(Error::IllegalAddress)
            } else if stat.failverify() {
                Err(Error::FailVerify)
            } else if stat.failmode() {
                Err(Error::FailMode)
            } else if stat.failinvdata() {
                Err(Error::FailInvData)
            } else if stat.failmisc() {
                Err(Error::Other)
            } else {
                Ok(())
            };
        };

        // TODO: ensure this is needed
        asm::dsb();
        asm::isb();

        ret
    }
}
/// Error type for Flash operations.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Operation using a location not in flash.
    OutOfBounds,
    /// Unaligned operation or using unaligned buffers.
    Unaligned,
    /// Dynamic Write/Erase protection violation.
    WriteProtected,
    /// Operation failed due to an illegal address.
    IllegalAddress,
    /// Failed to write within the pulse count limit.
    FailVerify,
    /// Failed because wrong mode configured (not all in READ mode).
    FailMode,
    /// Failed because of invalid data (transition from 0 to 1).
    FailInvData,
    /// Other error.
    Other,
}

impl From<NorFlashErrorKind> for Error {
    fn from(e: NorFlashErrorKind) -> Self {
        match e {
            NorFlashErrorKind::NotAligned => Self::Unaligned,
            NorFlashErrorKind::OutOfBounds => Self::OutOfBounds,
            _ => Self::Other,
        }
    }
}

impl NorFlashError for Error {
    fn kind(&self) -> NorFlashErrorKind {
        match self {
            Self::OutOfBounds => NorFlashErrorKind::OutOfBounds,
            Self::Unaligned => NorFlashErrorKind::NotAligned,
            _ => NorFlashErrorKind::Other,
        }
    }
}

impl ErrorType for Flash<'_> {
    type Error = Error;
}

impl ReadNorFlash for Flash<'_> {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Error> {
        check_read(self, offset, bytes.len())?;
        let flash_data =
            unsafe { core::slice::from_raw_parts((offset as usize) as *const u8, bytes.len()) };
        bytes.copy_from_slice(flash_data);
        Ok(())
    }

    fn capacity(&self) -> usize {
        // TODO: do this properly
        256 * 1024
    }
}

impl NorFlash for Flash<'_> {
    // TODO: Allow writing sub-words. Need to reckon with ECC implications...
    const WRITE_SIZE: usize = WORD_SIZE;
    const ERASE_SIZE: usize = 1024;

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Error> {
        check_write(self, offset, bytes.len())?;
        for (i, chunk) in bytes.chunks(Self::WRITE_SIZE).enumerate() {
            let addr = offset + (i as u32 * Self::WRITE_SIZE as u32);
            self.write_word(addr, chunk)?;
        }
        Ok(())
    }

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Error> {
        check_erase(self, from, to)?;
        for addr in (from..to).step_by(Self::ERASE_SIZE) {
            self.erase_sector(addr)?;
        }
        Ok(())
    }
}

#[inline(always)]
fn regs() -> crate::pac::flashctl::Flashctl {
    crate::pac::FLASHCTL
}
